use std::{
    collections::VecDeque,
    io,
    os::unix::process::ExitStatusExt,
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Margin, Rect},
};
use tui_term::vt100::{Callbacks, Parser, Screen};

use crate::{packages::Transaction, pty::Pty};

const SCROLLBACK_ROWS: usize = 2_000;

pub fn areas(area: Rect) -> (Rect, Rect, Rect) {
    let popup = area.inner(Margin {
        horizontal: if area.width >= 48 { 4 } else { 0 },
        vertical: if area.height >= 12 { 2 } else { 0 },
    });
    let [console, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)])
        .areas(popup.inner(Margin::new(1, 1)));
    (popup, console, footer)
}

pub struct TransactionTerminal {
    pub transaction: Transaction,
    parser: Parser<TerminalReplies>,
    runner: Runner,
    outcome: Option<String>,
}

enum Runner {
    Pending,
    Commands(CommandRun),
    AppImage(Receiver<ImageMessage>),
    Finished,
}

enum ImageMessage {
    Progress(String),
    Finished(Result<String, String>),
}

impl TransactionTerminal {
    pub fn new(transaction: Transaction, area: Rect) -> Self {
        let (rows, cols) = size(area);
        Self {
            transaction,
            parser: Parser::new_with_callbacks(
                rows,
                cols,
                SCROLLBACK_ROWS,
                TerminalReplies::default(),
            ),
            runner: Runner::Pending,
            outcome: None,
        }
    }

    pub fn screen(&self) -> &Screen {
        self.parser.screen()
    }

    pub fn is_running(&self) -> bool {
        !matches!(self.runner, Runner::Finished)
    }

    pub fn accepts_input(&self) -> bool {
        matches!(self.runner, Runner::Commands(_))
    }

    pub fn take_outcome(&mut self) -> Option<String> {
        self.outcome.take()
    }

    pub fn resize(&mut self, area: Rect) {
        let (rows, cols) = size(area);
        if (rows, cols) == self.screen().size() {
            return;
        }
        self.parser.screen_mut().set_size(rows, cols);
        if let Runner::Commands(run) = &self.runner
            && let Some((_, child)) = &run.current
            && let Err(error) = child.resize(rows, cols)
        {
            self.finish(Err(format!(
                "Could not resize the package terminal ({error})"
            )));
        }
    }

    pub fn poll(&mut self, mode: &libc::termios) -> bool {
        let (rows, cols) = self.screen().size();
        let mut output = Vec::new();
        let outcome = match &mut self.runner {
            Runner::Pending => {
                self.start();
                return true;
            }
            Runner::Commands(run) => run.poll(mode, rows, cols, &mut output),
            Runner::AppImage(receiver) => {
                let mut outcome = None;
                for _ in 0..64 {
                    match receiver.try_recv() {
                        Ok(ImageMessage::Progress(line)) => append_line(&mut output, &line),
                        Ok(ImageMessage::Finished(result)) => {
                            outcome = Some(result);
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            outcome =
                                Some(Err("The AppImage operation stopped unexpectedly".into()));
                            break;
                        }
                    }
                }
                outcome
            }
            Runner::Finished => return false,
        };

        self.parser.process(&output);
        let replies = std::mem::take(&mut self.parser.callbacks_mut().0);
        if let Some(outcome) = outcome {
            self.finish(outcome);
            return true;
        }
        if !replies.is_empty() {
            self.send(&replies);
        }
        !output.is_empty()
    }

    fn start(&mut self) {
        if let Transaction::AppImage(operation) = &self.transaction {
            let operation = operation.clone();
            let (sender, receiver) = mpsc::channel();
            let worker = thread::Builder::new()
                .name("appimage-operation".into())
                .spawn(move || {
                    let outcome = operation.run(|line| {
                        let _ = sender.send(ImageMessage::Progress(line.to_owned()));
                    });
                    let _ = sender.send(ImageMessage::Finished(outcome));
                });
            match worker {
                Ok(_) => self.runner = Runner::AppImage(receiver),
                Err(error) => {
                    self.finish(Err(format!("Could not start AppImage operation ({error})")))
                }
            }
            return;
        }

        match self.transaction.commands() {
            Ok(commands) if commands.is_empty() => {
                self.finish(Ok("No applications to update".into()))
            }
            Ok(commands) => self.runner = Runner::Commands(CommandRun::new(commands)),
            Err(error) => self.finish(Err(error)),
        }
    }

    fn finish(&mut self, outcome: Result<String, String>) {
        let message = match outcome {
            Ok(message) | Err(message) => message,
        };
        let mut output = b"\r\n\x1b[0m".to_vec();
        append_line(&mut output, &message);
        self.parser.process(&output);
        self.runner = Runner::Finished;
        self.outcome = Some(message);
    }

    pub fn handle_key(&mut self, key: KeyEvent, erase: u8) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let plain = key.modifiers.is_empty();
        if !self.is_running()
            && plain
            && matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q'))
        {
            return true;
        }

        let scroll = key.modifiers == KeyModifiers::SHIFT || (!self.is_running() && plain);
        if scroll && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            let screen = self.parser.screen_mut();
            let page = usize::from(screen.size().0.saturating_sub(1).max(1));
            let offset = if key.code == KeyCode::PageUp {
                screen.scrollback().saturating_add(page)
            } else {
                screen.scrollback().saturating_sub(page)
            };
            screen.set_scrollback(offset);
        } else if self.accepts_input() {
            let bytes = terminal_key(key, self.screen().application_cursor(), erase);
            if !bytes.is_empty() {
                self.parser.screen_mut().set_scrollback(0);
                self.send(&bytes);
            }
        }
        false
    }

    pub fn paste(&mut self, text: &str) {
        if !self.accepts_input() {
            return;
        }
        self.parser.screen_mut().set_scrollback(0);
        if self.screen().bracketed_paste() {
            let mut bytes = b"\x1b[200~".to_vec();
            bytes.extend_from_slice(text.as_bytes());
            bytes.extend_from_slice(b"\x1b[201~");
            self.send(&bytes);
        } else {
            self.send(text.as_bytes());
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Runner::Commands(run) = &mut self.runner
            && let Some((_, child)) = &mut run.current
            && let Err(error) = child.send(bytes)
        {
            self.finish(Err(format!(
                "Could not write to the package terminal ({error})"
            )));
        }
    }
}

fn size(area: Rect) -> (u16, u16) {
    let (_, console, _) = areas(area);
    (console.height.max(1), console.width.max(1))
}

struct CommandRun {
    pending: VecDeque<Command>,
    current: Option<(String, Pty)>,
    total: usize,
}

impl CommandRun {
    fn new(commands: Vec<Command>) -> Self {
        let total = commands.len();
        Self {
            pending: commands.into(),
            current: None,
            total,
        }
    }

    fn poll(
        &mut self,
        mode: &libc::termios,
        rows: u16,
        cols: u16,
        output: &mut Vec<u8>,
    ) -> Option<Result<String, String>> {
        if self.current.is_none() {
            let command = self.pending.pop_front()?;
            let program = command.get_program().to_string_lossy().into_owned();
            let step = self.total - self.pending.len();
            output.extend_from_slice(b"\x1b[0m\x1b[?1l\x1b[?2004l\x1b[?25h");
            if self.total > 1 {
                append_line(
                    output,
                    &format!("Running {program} ({step}/{})", self.total),
                );
            } else {
                append_line(output, &format!("Running {program}"));
            }
            match Pty::spawn(command, mode, rows, cols) {
                Ok(child) => self.current = Some((program, child)),
                Err(error) => {
                    let message = if error.kind() == io::ErrorKind::NotFound {
                        format!("Could not start {program} because its executable was not found")
                    } else {
                        format!("Could not start {program} ({error})")
                    };
                    return Some(Err(self.failure(message)));
                }
            }
        }

        let (program, child) = self.current.as_mut()?;
        match child.poll(output) {
            Ok(None) => None,
            Ok(Some(status)) if status.success() && !self.pending.is_empty() => {
                self.current = None;
                output.extend_from_slice(b"\r\n");
                None
            }
            Ok(Some(status)) if status.success() && self.total > 1 => Some(Ok(format!(
                "All {} commands completed successfully",
                self.total
            ))),
            Ok(Some(status)) if status.success() => {
                Some(Ok(format!("{program} completed successfully")))
            }
            Ok(Some(status)) => {
                let message = if status.signal() == Some(libc::SIGINT) {
                    format!("{program} was interrupted")
                } else {
                    format!("{program} failed ({status})")
                };
                Some(Err(self.failure(message)))
            }
            Err(error) => {
                let message = format!("Could not read the {program} terminal ({error})");
                Some(Err(self.failure(message)))
            }
        }
    }

    fn failure(&self, message: String) -> String {
        let completed = self.total - self.pending.len() - 1;
        if completed > 0 {
            format!(
                "Completed {completed} of {} commands. {message}",
                self.total
            )
        } else {
            message
        }
    }
}

fn append_line(output: &mut Vec<u8>, text: &str) {
    let clean: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    output.extend_from_slice(clean.as_bytes());
    output.extend_from_slice(b"\r\n");
}

#[derive(Default)]
struct TerminalReplies(Vec<u8>);

impl Callbacks for TerminalReplies {
    fn unhandled_csi(
        &mut self,
        screen: &mut Screen,
        intermediate: Option<u8>,
        second: Option<u8>,
        params: &[&[u16]],
        command: char,
    ) {
        let param = params
            .first()
            .and_then(|values| values.first())
            .copied()
            .unwrap_or(0);
        match (intermediate, second, command, param) {
            (None, None, 'n', 5) => self.0.extend_from_slice(b"\x1b[0n"),
            (prefix @ (None | Some(b'?')), None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                let prefix = if prefix.is_some() { "?" } else { "" };
                self.0
                    .extend_from_slice(format!("\x1b[{prefix}{};{}R", row + 1, col + 1).as_bytes());
            }
            _ => {}
        }
    }
}

fn terminal_key(key: KeyEvent, application_cursor: bool, erase: u8) -> Vec<u8> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
    let cursor = |letter| {
        if modifier != 1 {
            format!("\x1b[1;{modifier}{letter}")
        } else if application_cursor {
            format!("\x1bO{letter}")
        } else {
            format!("\x1b[{letter}")
        }
    };
    let numbered = |number| {
        if modifier == 1 {
            format!("\x1b[{number}~")
        } else {
            format!("\x1b[{number};{modifier}~")
        }
    };
    let with_alt = |mut bytes: Vec<u8>| {
        if alt {
            bytes.insert(0, b'\x1b');
        }
        bytes
    };
    match key.code {
        KeyCode::Char(character) if control => {
            let character = character.to_ascii_uppercase();
            match character {
                '@'..='_' => with_alt(vec![character as u8 & 0x1f]),
                ' ' => with_alt(vec![0]),
                '?' => with_alt(vec![0x7f]),
                _ => Vec::new(),
            }
        }
        KeyCode::Char(character) => {
            with_alt(character.encode_utf8(&mut [0; 4]).as_bytes().to_vec())
        }
        KeyCode::Enter => with_alt(vec![b'\r']),
        KeyCode::Backspace => with_alt(vec![erase]),
        KeyCode::Tab if !shift => with_alt(vec![b'\t']),
        KeyCode::Tab | KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => with_alt(vec![b'\x1b']),
        KeyCode::Up => cursor('A').into_bytes(),
        KeyCode::Down => cursor('B').into_bytes(),
        KeyCode::Right => cursor('C').into_bytes(),
        KeyCode::Left => cursor('D').into_bytes(),
        KeyCode::Home => cursor('H').into_bytes(),
        KeyCode::End => cursor('F').into_bytes(),
        KeyCode::Insert => numbered(2).into_bytes(),
        KeyCode::Delete => numbered(3).into_bytes(),
        KeyCode::PageUp => numbered(5).into_bytes(),
        KeyCode::PageDown => numbered(6).into_bytes(),
        KeyCode::F(index @ 1..=4) => {
            let letter = char::from(b'P' + index - 1);
            if modifier == 1 {
                format!("\x1bO{letter}").into_bytes()
            } else {
                format!("\x1b[1;{modifier}{letter}").into_bytes()
            }
        }
        KeyCode::F(index @ 5..=12) => {
            numbered([15, 17, 18, 19, 20, 21, 23, 24][usize::from(index - 5)]).into_bytes()
        }
        KeyCode::Null => vec![0],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_prompt_input_and_terminal_navigation() {
        let interrupt = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(terminal_key(interrupt, false, 0x7f), b"\x03");
        assert_eq!(
            terminal_key(KeyEvent::from(KeyCode::Enter), false, 0x7f),
            b"\r"
        );
        assert_eq!(
            terminal_key(KeyEvent::from(KeyCode::Backspace), false, 0x08),
            b"\x08"
        );
        assert_eq!(
            terminal_key(KeyEvent::from(KeyCode::Char('é')), false, 0x7f),
            "é".as_bytes()
        );
        assert_eq!(
            terminal_key(KeyEvent::from(KeyCode::Up), true, 0x7f),
            b"\x1bOA"
        );
        let alt_right = KeyEvent::new(KeyCode::Right, KeyModifiers::ALT);
        assert_eq!(terminal_key(alt_right, false, 0x7f), b"\x1b[1;3C");
    }

    #[test]
    fn retains_output_and_allows_closing_after_completion() {
        let mut popup =
            TransactionTerminal::new(Transaction::Upgrade { aur: false }, Rect::new(0, 0, 80, 24));
        let enter = KeyEvent::from(KeyCode::Enter);
        assert!(!popup.handle_key(enter, 0x7f));
        popup.parser.process(b"transaction output\r\n");
        popup.finish(Err("Command failed".into()));

        assert!(!popup.is_running());
        assert!(popup.screen().contents().contains("transaction output"));
        assert!(popup.screen().contents().contains("Command failed"));
        assert!(popup.handle_key(enter, 0x7f));
        assert_eq!(popup.take_outcome().as_deref(), Some("Command failed"));
        assert!(popup.take_outcome().is_none());
    }

    #[test]
    fn responds_to_cursor_and_device_status_queries() {
        let mut parser = Parser::new_with_callbacks(12, 40, 0, TerminalReplies::default());
        parser.process(b"\x1b[3;7H\x1b[6n\x1b[5n");
        assert_eq!(parser.callbacks().0, b"\x1b[3;7R\x1b[0n");
    }
}
