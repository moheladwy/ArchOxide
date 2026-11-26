use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::*;

#[test]
fn flatpak_actions_preserve_installation_and_cancel_a_pending_search() {
    let mut app = App {
        view: View::Flatpak,
        ..App::default()
    };
    let user = flatpak::InstalledApp {
        id: "org.example.Demo".into(),
        name: "Demo".into(),
        version: "1".into(),
        reference: "app/org.example.Demo/x86_64/stable".into(),
        remote: "flathub".into(),
        installation: flatpak::Installation::User,
    };
    let system = flatpak::InstalledApp {
        installation: flatpak::Installation::System,
        ..user.clone()
    };
    assert!(app.apply_source_read(Response::FlatpakInventory(
        0,
        Ok(vec![user.clone(), system])
    )));
    app.apply_source_action(Action::ToggleMark);
    app.apply_source_action(Action::UpgradeExcept);
    assert!(
        matches!(&app.confirmation, Some(Transaction::Flatpak(flatpak::Operation::UpdateExcept(excluded))) if excluded == std::slice::from_ref(&user))
    );
    app.handle_key(KeyEvent::from(KeyCode::Esc), Instant::now());
    assert!(app.take_transaction().is_none());

    app.handle_key(KeyEvent::from(KeyCode::Char('j')), Instant::now());
    assert_eq!(
        app.selected_flatpak().unwrap().installation,
        flatpak::Installation::System
    );
    app.handle_key(KeyEvent::from(KeyCode::Char('d')), Instant::now());
    app.handle_key(KeyEvent::from(KeyCode::Enter), Instant::now());
    assert!(
        matches!(app.take_transaction(), Some(Transaction::Flatpak(flatpak::Operation::Remove(apps))) if apps == [user])
    );

    app.sources.flatpak_query = Some("demo".into());
    app.handle_key(KeyEvent::from(KeyCode::Char('d')), Instant::now());
    assert!(app.confirmation.is_none());
    app.leave_flatpak_search();
    assert!(!app.apply_source_read(Response::FlatpakSearch(0, Ok(Vec::new()))));
    assert_eq!(app.flatpak_rows().count(), 2);
}
