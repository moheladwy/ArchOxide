use std::time::Instant;

use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent},
};

use super::*;

fn render(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn details_pane_appears_only_when_it_fits() {
    let mut app = App::default();
    let wide = render(&mut app, 100, 24);
    assert!(wide.contains("ArchOxide"));
    assert!(wide.contains("Filter installed"));
    assert!(wide.contains("Details"));

    let narrow = render(&mut app, 40, 14);
    assert!(narrow.contains("Packages"));
    assert!(narrow.contains("? Help"));
    assert!(!narrow.contains("Details"));

    let tiny = render(&mut app, 12, 2);
    assert!(tiny.contains("ArchOxide"));
}

#[test]
fn disabled_sources_still_show_switches_in_settings() {
    let mut config = crate::config::Config::default();
    config.packages.flatpak_enabled = false;
    config.packages.appimage_enabled = false;
    let mut app = App::new(config, None, None);
    app.view = View::Settings;

    let screen = render(&mut app, 100, 24);
    let header = screen.lines().next().unwrap();
    assert!(!header.contains("Flatpak"));
    assert!(!header.contains("AppImages"));
    for label in ["Flatpak support", "AppImages support"] {
        assert!(
            screen
                .lines()
                .any(|line| line.contains(label) && line.contains("Off"))
        );
    }
}

#[test]
fn help_overlays_content_and_closes_with_escape() {
    let mut app = App::default();
    let now = Instant::now();

    app.handle_key(KeyEvent::from(KeyCode::Char('?')), now);
    let help = render(&mut app, 90, 25);
    assert!(help.contains("Move through rows"));
    assert!(help.contains("Esc Close help"));

    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    let restored = render(&mut app, 90, 25);
    assert!(restored.contains("Reading installed packages"));
    assert!(!restored.contains("Esc Close help"));
}

#[test]
fn saved_theme_and_layout_control_the_rendered_view() {
    let mut app = App::default();
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let dark_background = terminal.backend().buffer()[(99, 0)].bg;
    assert_eq!(terminal.backend().buffer()[(60, 4)].symbol(), "┌");

    app.config.appearance.theme = Theme::Light;
    app.config.appearance.details_width_percent = 50;
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_ne!(buffer[(99, 0)].bg, dark_background);
    assert_eq!(buffer[(50, 4)].symbol(), "┌");

    let palette = Palette::new(Theme::Light);
    assert_eq!(
        package_row("test", "1", "extra", false, true, palette).height(),
        2
    );
    assert_eq!(
        package_row("test", "1", "extra", false, false, palette).height(),
        1
    );
}
