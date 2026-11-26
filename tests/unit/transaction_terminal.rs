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
