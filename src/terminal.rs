use std::{
    io::{self, IsTerminal, stdin, stdout},
    mem::MaybeUninit,
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

use ratatui::{
    DefaultTerminal,
    crossterm::{
        cursor::Show,
        event::{
            self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyboardEnhancementFlags,
            PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
        },
        execute,
        terminal::{LeaveAlternateScreen, disable_raw_mode, supports_keyboard_enhancement},
    },
    layout::Rect,
};

use crate::{app::App, transaction_terminal::TransactionTerminal, ui};

const READ_POLL_INTERVAL: Duration = Duration::from_millis(50);
const TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(20);

pub fn run(mut app: App) -> io::Result<()> {
    if !stdin().is_terminal() || !stdout().is_terminal() {
        return Err(io::Error::other(
            "open ArchOxide in an interactive terminal",
        ));
    }

    let original_mode = terminal_mode()?;
    let mut enhanced_keyboard = false;
    let result = ratatui::try_init().and_then(|mut terminal| {
        app.start_background_reads();
        if supports_keyboard_enhancement().unwrap_or(false) {
            execute!(
                stdout(),
                PushKeyboardEnhancementFlags(
                    // Terminals such as Kitty keep Enter in legacy form without the all-keys flag.
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                        // Include the keyboard layout's shifted character in encoded key events.
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
                )
            )?;
            enhanced_keyboard = true;
            app.enable_key_event_reporting();
        }
        run_loop(&mut terminal, &mut app, &original_mode)
    });

    let cleanup = restore_terminal(enhanced_keyboard);
    let mode = apply_terminal_mode(&original_mode);
    result.and(cleanup).and(mode)
}

fn restore_terminal(enhanced_keyboard: bool) -> io::Result<()> {
    // Restore the keyboard flags before leaving the screen whose stack we used.
    let keyboard = if enhanced_keyboard {
        execute!(stdout(), PopKeyboardEnhancementFlags)
    } else {
        Ok(())
    };
    let raw_mode = disable_raw_mode();
    let screen = execute!(stdout(), DisableBracketedPaste, LeaveAlternateScreen, Show);
    keyboard.and(raw_mode).and(screen)
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    original_mode: &libc::termios,
) -> io::Result<()> {
    let mut needs_draw = true;
    let mut popup: Option<TransactionTerminal> = None;
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
        if let Some(popup) = &mut popup {
            needs_draw |= popup.poll(original_mode);
            if let Some(message) = popup.take_outcome() {
                app.transaction_finished(message, &popup.transaction);
                needs_draw = true;
            }
        }
        if needs_draw {
            terminal.draw(|frame| {
                ui::draw(frame, app);
                if let Some(popup) = &popup {
                    ui::draw_package_terminal(frame, app, popup);
                }
            })?;
            needs_draw = false;
        }

        let mut timeout = app.bindings.remaining(Instant::now());
        let poll_interval = if popup.as_ref().is_some_and(TransactionTerminal::is_running) {
            Some(TERMINAL_POLL_INTERVAL)
        } else if app.has_pending_reads() {
            Some(READ_POLL_INTERVAL)
        } else {
            None
        };
        if let Some(interval) = poll_interval {
            timeout = Some(timeout.map_or(interval, |time| time.min(interval)));
        }
        if let Some(timeout) = timeout
            && !event::poll(timeout)?
        {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if let Some(display) = &mut popup {
                    if display.handle_key(key, original_mode.c_cc[libc::VERASE]) {
                        popup = None;
                        execute!(stdout(), DisableBracketedPaste)?;
                    }
                } else {
                    app.handle_key(key, Instant::now());
                    if let Some(transaction) = app.take_transaction() {
                        popup = Some(TransactionTerminal::new(
                            transaction,
                            terminal.size()?.into(),
                        ));
                        execute!(stdout(), EnableBracketedPaste)?;
                    }
                }
                needs_draw = true;
            }
            Event::Paste(text) => {
                if let Some(popup) = &mut popup {
                    popup.paste(&text);
                    needs_draw = true;
                }
            }
            Event::Resize(width, height) => {
                if let Some(popup) = &mut popup {
                    popup.resize(Rect::new(0, 0, width, height));
                }
                needs_draw = true;
            }
            _ => {}
        }
    }
    Ok(())
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
