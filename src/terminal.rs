use std::{
    io::{self, IsTerminal, stdin, stdout},
    time::Instant,
};

use ratatui::{
    DefaultTerminal,
    crossterm::{
        cursor::Show,
        event::{self, Event},
        execute,
        terminal::{LeaveAlternateScreen, disable_raw_mode},
    },
};

use crate::{app::App, ui};

pub fn run() -> io::Result<()> {
    if !stdin().is_terminal() || !stdout().is_terminal() {
        return Err(io::Error::other(
            "open ArchOxide in an interactive terminal",
        ));
    }

    let result = ratatui::try_init().and_then(|mut terminal| run_loop(&mut terminal));
    // Attempt both cleanup operations even if setup or raw-mode restoration fails.
    let raw_mode = disable_raw_mode();
    let screen = execute!(stdout(), LeaveAlternateScreen, Show);
    result.and(raw_mode).and(screen)
}

fn run_loop(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut app = App::default();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
        if let Some(timeout) = app.bindings.remaining(Instant::now())
            && !event::poll(timeout)?
        {
            app.bindings.expire(Instant::now());
            continue;
        }
        if let Event::Key(key) = event::read()? {
            app.handle_key(key, Instant::now());
        }
    }
    Ok(())
}
