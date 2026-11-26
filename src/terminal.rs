use std::{
    io::{self, IsTerminal, stdin, stdout},
    mem::MaybeUninit,
    os::{fd::AsRawFd, unix::process::ExitStatusExt},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use ratatui::{
    DefaultTerminal,
    crossterm::{
        cursor::Show,
        event::{self, Event},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

use crate::{app::App, packages::Transaction, ui};

const READ_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn run(mut app: App) -> io::Result<()> {
    if !stdin().is_terminal() || !stdout().is_terminal() {
        return Err(io::Error::other(
            "open ArchOxide in an interactive terminal",
        ));
    }

    let original_mode = terminal_mode()?;
    let result = ratatui::try_init().and_then(|mut terminal| {
        app.start_background_reads();
        run_loop(&mut terminal, &mut app, &original_mode)
    });

    let cleanup = restore_terminal();
    let mode = apply_terminal_mode(&original_mode);
    result.and(cleanup).and(mode)
}

fn restore_terminal() -> io::Result<()> {
    // Attempt both cleanup operations even if setup or raw-mode restoration fails.
    let raw_mode = disable_raw_mode();
    let screen = execute!(stdout(), LeaveAlternateScreen, Show);
    raw_mode.and(screen)
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    original_mode: &libc::termios,
) -> io::Result<()> {
    let mut needs_draw = true;
    while !app.should_quit {
        let now = Instant::now();
        if app
            .bindings
            .remaining(now)
            .is_some_and(|time| time.is_zero())
        {
            app.bindings.expire(now);
            needs_draw = true;
        }
        needs_draw |= app.poll_reads();
        if needs_draw {
            terminal.draw(|frame| ui::draw(frame, app))?;
            needs_draw = false;
        }

        let mut timeout = app.bindings.remaining(Instant::now());
        if app.has_pending_reads() {
            timeout = Some(timeout.map_or(READ_POLL_INTERVAL, |time| time.min(READ_POLL_INTERVAL)));
        }
        if let Some(timeout) = timeout
            && !event::poll(timeout)?
        {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                app.handle_key(key, Instant::now());
                if let Some(transaction) = app.take_transaction() {
                    run_transaction(terminal, app, transaction, original_mode)?;
                }
                needs_draw = true;
            }
            Event::Resize(_, _) => needs_draw = true,
            _ => {}
        }
    }
    Ok(())
}

fn run_transaction(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    transaction: Transaction,
    original_mode: &libc::termios,
) -> io::Result<()> {
    let mut command = match transaction.command() {
        Ok(command) => command,
        Err(message) => {
            app.transaction_finished(message);
            return Ok(());
        }
    };

    // Terminal signals go to the foreground process group, including ArchOxide.
    // Catch them for the handoff so exec can give the child its default handlers.
    let _signal_handlers = match InterruptHandler::catch(libc::SIGINT)
        .and_then(|interrupt| InterruptHandler::catch(libc::SIGQUIT).map(|quit| (interrupt, quit)))
    {
        Ok(handlers) => handlers,
        Err(error) => {
            app.transaction_finished(format!("Could not prepare the terminal handoff ({error})"));
            return Ok(());
        }
    };

    let outcome = match restore_terminal() {
        Ok(()) => command_outcome(&mut command),
        Err(error) => format!("Could not hand off the terminal ({error})"),
    };
    let resumed = resume_terminal(terminal, original_mode);
    app.transaction_finished(outcome);
    resumed
}

fn resume_terminal(
    terminal: &mut DefaultTerminal,
    original_mode: &libc::termios,
) -> io::Result<()> {
    // A child that exits during a prompt can leave different terminal settings.
    apply_terminal_mode(original_mode)?;
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    // Reset Ratatui's previous frame and pick up any resize during the transaction.
    terminal.resize(terminal.size()?.into())
}

fn command_outcome(command: &mut Command) -> String {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = match command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return format!("Could not start {program} because its executable was not found");
        }
        Err(error) => return format!("Could not start {program} ({error})"),
    };
    match child.wait() {
        Ok(status) if status.success() => format!("{program} completed successfully"),
        Ok(status) if status.signal() == Some(libc::SIGINT) => format!("{program} was interrupted"),
        Ok(status) => format!("{program} failed ({status})"),
        Err(error) => format!("Could not wait for {program} ({error})"),
    }
}

fn terminal_mode() -> io::Result<libc::termios> {
    let mut mode = MaybeUninit::uninit();
    // SAFETY: tcgetattr initializes the termios value on success.
    if unsafe { libc::tcgetattr(stdin().as_raw_fd(), mode.as_mut_ptr()) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { mode.assume_init() })
}

fn apply_terminal_mode(mode: &libc::termios) -> io::Result<()> {
    // SAFETY: mode refers to the settings read from stdin by tcgetattr.
    if unsafe { libc::tcsetattr(stdin().as_raw_fd(), libc::TCSANOW, mode) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

struct InterruptHandler {
    signal: libc::c_int,
    previous: libc::sigaction,
}

impl InterruptHandler {
    fn catch(signal: libc::c_int) -> io::Result<Self> {
        extern "C" fn handle_interrupt(_: libc::c_int) {}

        // SAFETY: sigaction can be zero-initialized before setting its handler and mask.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = handle_interrupt as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        let mut previous = MaybeUninit::uninit();
        // SAFETY: The handler does nothing, and sigaction writes previous on success.
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(signal, &action, previous.as_mut_ptr()) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                signal,
                previous: previous.assume_init(),
            })
        }
    }
}

impl Drop for InterruptHandler {
    fn drop(&mut self) {
        // SAFETY: Restore the handler returned by sigaction for this signal.
        unsafe { libc::sigaction(self.signal, &self.previous, std::ptr::null_mut()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_foreground_command_outcomes() {
        let mut success = Command::new("sh");
        success.args(["-c", "exit 0"]);
        assert_eq!(command_outcome(&mut success), "sh completed successfully");

        let mut failure = Command::new("sh");
        failure.args(["-c", "exit 7"]);
        assert_eq!(command_outcome(&mut failure), "sh failed (exit status: 7)");

        let mut interrupt = Command::new("sh");
        interrupt.args(["-c", "kill -INT $$"]);
        assert_eq!(command_outcome(&mut interrupt), "sh was interrupted");

        let directory = tempfile::tempdir().unwrap();
        let mut missing = Command::new(directory.path().join("missing-command"));
        assert!(command_outcome(&mut missing).contains("executable was not found"));
    }
}
