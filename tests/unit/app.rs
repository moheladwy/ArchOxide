use std::time::Duration;

use super::*;

fn press(app: &mut App, keys: &str, now: Instant) {
    for character in keys.chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(character)), now);
    }
}

#[test]
fn leader_opens_theme_explorer_and_quits() {
    let mut app = App::default();
    let now = Instant::now();

    press(&mut app, " ft", now);
    assert_eq!(app.view, View::Settings);
    assert_eq!(app.selection_mut().selected(), Some(0));
    press(&mut app, " e", now);
    assert_eq!(app.view, View::Installed);
    assert_eq!(app.selection_mut().selected(), None);
    press(&mut app, " q", now);
    assert!(app.should_quit);
}

#[test]
fn expired_and_cancelled_sequences_do_not_trigger_actions() {
    let mut app = App::default();
    let now = Instant::now();

    press(&mut app, " f", now);
    press(&mut app, "f", now + Duration::from_millis(700));
    assert_eq!(app.view, View::Installed);

    press(&mut app, " ", now);
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    press(&mut app, "q", now);
    assert!(!app.should_quit);
    assert!(app.bindings.pending_label().is_none());
}

#[test]
fn prompt_treats_normal_mode_bindings_as_text_and_restores_cancelled_edits() {
    let mut app = App::default();
    let now = Instant::now();

    press(&mut app, " ff", now);
    assert_eq!(app.view, View::Search);
    assert!(app.editing());
    press(&mut app, "jk? qé", now);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert_eq!(app.prompt(), Some(("Search packages", "jk? qé")));
    assert!(!app.editing());
    assert!(!app.show_help);
    assert!(!app.should_quit);

    press(&mut app, "/", now);
    app.handle_key(KeyEvent::from(KeyCode::Backspace), now);
    assert_eq!(app.prompt(), Some(("Search packages", "jk? q")));
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    assert_eq!(app.prompt(), Some(("Search packages", "jk? qé")));
}

#[test]
fn row_navigation_clamps_to_available_items() {
    let mut app = App::default();
    let now = Instant::now();

    press(&mut app, " ft", now);
    app.update_layout(6, true);
    app.handle_key(
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        now,
    );
    assert_eq!(app.selection_mut().selected(), Some(3));
    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL), now);
    assert_eq!(app.selection_mut().selected(), Some(Setting::ALL.len() - 1));
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL), now);
    assert_eq!(app.selection_mut().selected(), Some(0));
    press(&mut app, "G", now);
    assert_eq!(app.selection_mut().selected(), Some(Setting::ALL.len() - 1));
    app.navigate(Action::Down, 2);
    assert_eq!(app.selection_mut().selected(), Some(1));
    press(&mut app, "ggk", now);
    assert_eq!(app.selection_mut().selected(), Some(0));
    app.navigate(Action::Last, 0);
    assert_eq!(app.selection_mut().selected(), None);
}

#[test]
fn tab_variants_cycle_views_and_resize_resets_hidden_focus() {
    let mut app = App::default();
    let now = Instant::now();

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::ALT), now);
    assert_eq!(app.view, View::Search);
    app.handle_key(
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::ALT | KeyModifiers::SHIFT),
        now,
    );
    assert_eq!(app.view, View::Installed);
    app.handle_key(KeyEvent::from(KeyCode::BackTab), now);
    assert_eq!(app.view, View::Settings);

    app.update_layout(10, true);
    press(&mut app, "l", now);
    assert_eq!(app.focus, Pane::Details);
    app.update_layout(10, false);
    assert_eq!(app.focus, Pane::List);
    press(&mut app, "l", now);
    assert_eq!(app.focus, Pane::List);
}

fn installed_package(name: &str, version: &str, availability: Availability) -> InstalledPackage {
    InstalledPackage {
        name: name.into(),
        version: version.into(),
        availability,
    }
}

fn search_result(name: &str, source: Source) -> SearchResult {
    SearchResult {
        name: name.into(),
        version: "2.0-1".into(),
        source,
        description: String::new(),
    }
}

#[test]
fn installed_filter_combines_live_text_and_availability() {
    let mut app = App::default();
    app.apply_read(Response::Inventory(
        0,
        Ok(Inventory {
            packages: vec![
                installed_package("ripgrep-git", "2.0.r1", Availability::Foreign),
                installed_package("ripgrep", "2.0", Availability::Repository),
                installed_package("fd", "1.4", Availability::Repository),
            ],
            warning: None,
        }),
    ));
    let now = Instant::now();

    press(&mut app, "/RIPGREP", now);
    assert_eq!(app.installed_rows().count(), 2);
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    assert_eq!(app.installed_rows().count(), 3);

    press(&mut app, "aa/2.0", now);
    assert_eq!(app.installed_rows().count(), 1);
    assert_eq!(app.selected_installed().unwrap().name, "ripgrep-git");
    assert_eq!(app.installed_version("ripgrep"), Some("2.0"));
}

#[test]
fn search_keeps_repository_results_when_aur_fails_and_discards_stale_response() {
    let mut app = App {
        search_id: 2,
        ..App::default()
    };
    let repo = search_result("archoxide", Source::Repository("extra".into()));
    let stale = search_result("unrelated", Source::Aur);

    assert!(app.apply_read(Response::Search(
        SearchSource::Repository,
        2,
        Ok(vec![repo.clone()])
    )));
    assert!(!app.apply_read(Response::Search(SearchSource::Aur, 1, Ok(vec![stale]))));
    assert!(app.apply_read(Response::Search(
        SearchSource::Aur,
        2,
        Err("AUR unavailable".into())
    )));

    assert_eq!(app.search_rows().collect::<Vec<_>>(), [&repo]);
    assert_eq!(
        app.search_states[1],
        ReadState::Failed("AUR unavailable".into())
    );
}

#[test]
fn search_preserves_the_selected_source_when_repository_rows_arrive_later() {
    let mut app = App {
        search_states: [
            ReadState::Loading,
            ReadState::Loading,
            ReadState::Idle,
            ReadState::Idle,
        ],
        ..App::default()
    };
    let repo = search_result("same-name", Source::Repository("extra".into()));
    let aur = search_result("same-name", Source::Aur);

    app.apply_read(Response::Search(
        SearchSource::Aur,
        0,
        Ok(vec![aur.clone()]),
    ));
    assert_eq!(app.selected_result(), Some(&aur));

    app.apply_read(Response::Search(
        SearchSource::Repository,
        0,
        Ok(vec![repo.clone()]),
    ));
    assert_eq!(app.search_rows().collect::<Vec<_>>(), [&repo, &aur]);
    assert_eq!(app.selected_result(), Some(&aur));
}

#[test]
fn package_actions_require_confirmation_and_keep_the_selected_target() {
    let mut app = App::default();
    app.apply_read(Response::Inventory(
        0,
        Ok(Inventory {
            packages: vec![installed_package(
                "ripgrep",
                "2.0",
                Availability::Repository,
            )],
            warning: None,
        }),
    ));
    let now = Instant::now();

    press(&mut app, "d", now);
    assert!(app.take_transaction().is_none());
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    assert!(app.take_transaction().is_none());

    press(&mut app, "d", now);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert!(
        matches!(app.take_transaction(), Some(Transaction::Remove(package)) if package.name == "ripgrep")
    );

    app.yay_available = false;
    press(&mut app, " u", now);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert!(matches!(
        app.take_transaction(),
        Some(Transaction::Upgrade { aur: false })
    ));
}

#[test]
fn scoped_upgrades_use_marks_across_filters_or_the_highlighted_row() {
    let mut app = App::default();
    let inventory = vec![
        installed_package("alpha", "1", Availability::Repository),
        installed_package("beta", "1", Availability::Foreign),
        installed_package("gamma", "1", Availability::Repository),
    ];
    app.apply_read(Response::Inventory(
        0,
        Ok(Inventory {
            packages: inventory,
            warning: None,
        }),
    ));
    let now = Instant::now();

    press(&mut app, "u", now);
    assert!(
        matches!(&app.confirmation, Some(Transaction::UpgradeSelected { aur: false, names }) if names == &["alpha"])
    );
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    assert!(app.take_transaction().is_none());

    press(&mut app, "xjx/gamma", now);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert_eq!(app.marked_count(), 2);
    press(&mut app, "u", now);
    assert!(app.confirmation.is_none());
    assert!(app.notice.as_deref().unwrap().contains("requires yay"));
    app.yay_available = true;
    press(&mut app, "u", now);
    assert!(
        matches!(&app.confirmation, Some(Transaction::UpgradeSelected { aur: true, names }) if names == &["alpha", "beta"])
    );
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);

    press(&mut app, "U", now);
    assert!(
        matches!(&app.confirmation, Some(Transaction::UpgradeExcept { aur: true, names }) if names == &["alpha", "beta"])
    );
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    press(&mut app, " u", now);
    assert!(matches!(
        &app.confirmation,
        Some(Transaction::Upgrade { aur: true })
    ));
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    press(&mut app, "XU", now);
    assert!(
        matches!(&app.confirmation, Some(Transaction::UpgradeExcept { aur: true, names }) if names == &["gamma"])
    );
}

#[test]
fn settings_are_applied_only_after_save_and_reset_is_a_draft() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("archoxide/config.toml");
    let mut app = App::new(Config::default(), Some(path.clone()), None);
    let now = Instant::now();

    press(&mut app, " ft", now);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    press(&mut app, "j", now);
    assert_eq!(app.rendered_theme(), Theme::Light);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert_eq!(app.setting_value(Setting::Theme), "Light");
    assert_eq!(app.config.appearance.theme, Theme::Dark);
    assert!(!path.exists());

    press(&mut app, "s", now);
    assert_eq!(app.config.appearance.theme, Theme::Light);
    assert_eq!(Config::load(&path).unwrap(), app.config);
    press(&mut app, "r", now);
    assert_eq!(app.setting_value(Setting::Theme), "Dark");
    assert_eq!(Config::load(&path).unwrap().appearance.theme, Theme::Light);
}

#[test]
fn searchable_font_picker_saves_a_family_from_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let mut app = App::new(Config::default(), Some(path.clone()), None);
    app.view = View::Settings;
    app.picker = Some(Picker::fonts());
    assert!(app.apply_read(Response::Fonts(
        0,
        Ok(vec!["Adwaita Sans".into(), "DejaVu Sans Mono".into()])
    )));
    let now = Instant::now();

    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL), now);
    assert_eq!(app.picker.as_ref().unwrap().selection.selected(), Some(2));
    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL), now);
    assert_eq!(app.picker.as_ref().unwrap().selection.selected(), Some(0));
    press(&mut app, "/dejavu", now);
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert_eq!(app.setting_value(Setting::FontFamily), "DejaVu Sans Mono");
    assert!(app.config.appearance.font_family.is_none());
    press(&mut app, "s", now);
    assert_eq!(
        Config::load(&path)
            .unwrap()
            .appearance
            .font_family
            .as_deref(),
        Some("DejaVu Sans Mono")
    );
}

#[test]
fn disabled_sources_are_persisted_and_skipped_by_navigation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let mut app = App::new(Config::default(), Some(path.clone()), None);
    let now = Instant::now();

    press(&mut app, " ftjjjjjj", now);
    assert_eq!(app.selected_setting(), Some(Setting::Flatpak));
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    press(&mut app, "j", now);
    assert_eq!(app.selected_setting(), Some(Setting::AppImages));
    app.handle_key(KeyEvent::from(KeyCode::Enter), now);
    assert_eq!(app.setting_value(Setting::Flatpak), "Off");
    assert_eq!(app.setting_value(Setting::AppImages), "Off");
    assert!(app.views().any(|view| view == View::Flatpak));

    press(&mut app, "s", now);
    let loaded = Config::load(&path).unwrap();
    assert!(!loaded.packages.flatpak_enabled);
    assert!(!loaded.packages.appimage_enabled);
    assert!(loaded.packages.aur_enabled);
    assert_eq!(
        app.views().collect::<Vec<_>>(),
        [View::Installed, View::Search, View::Apps, View::Settings]
    );
    assert!(!app.apply_read(Response::FlatpakInventory(0, Ok(Vec::new()))));
    assert!(!app.apply_read(Response::ImageInventory(0, Ok(Vec::new()))));
    assert!(!app.has_pending_reads());

    press(&mut app, " p a", now);
    assert_eq!(app.view, View::Settings);
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("AppImages support is off")
    );
    app.handle_key(KeyEvent::from(KeyCode::BackTab), now);
    assert_eq!(app.view, View::Apps);
    app.handle_key(KeyEvent::from(KeyCode::BackTab), now);
    assert_eq!(app.view, View::Search);
    app.handle_key(KeyEvent::from(KeyCode::Tab), now);
    assert_eq!(app.view, View::Apps);
    app.handle_key(KeyEvent::from(KeyCode::Tab), now);
    assert_eq!(app.view, View::Settings);

    press(&mut app, "rs", now);
    assert_eq!(app.views().collect::<Vec<_>>(), View::ALL);
}

#[test]
fn unified_search_preserves_sources_and_dispatches_installs() {
    let mut app = App {
        view: View::Search,
        search_id: 3,
        ..App::default()
    };
    let repo = search_result("demo", Source::Repository("extra".into()));
    let aur = search_result("demo", Source::Aur);
    let flatpak = search_result(
        "demo",
        Source::Flatpak {
            installation: crate::flatpak::Installation::User,
            remote: "flathub".into(),
            reference: "app/org.example.Demo/x86_64/stable".into(),
        },
    );
    let image = search_result(
        "demo",
        Source::AppImageImport("https://example.org/demo.AppImage".into()),
    );
    assert!(app.apply_read(Response::Search(
        SearchSource::Flatpak,
        3,
        Ok(vec![flatpak.clone()])
    )));
    assert_eq!(app.selected_result(), Some(&flatpak));
    assert!(!app.apply_read(Response::Search(
        SearchSource::AppImage,
        2,
        Ok(vec![image.clone()])
    )));
    assert!(app.apply_read(Response::Search(
        SearchSource::AppImage,
        3,
        Ok(vec![image.clone()])
    )));
    assert!(app.apply_read(Response::Search(
        SearchSource::Repository,
        3,
        Ok(vec![repo.clone()])
    )));
    assert!(app.apply_read(Response::Search(
        SearchSource::Aur,
        3,
        Ok(vec![aur.clone()])
    )));
    assert_eq!(
        app.search_rows().collect::<Vec<_>>(),
        [&repo, &aur, &flatpak, &image]
    );
    assert_eq!(app.selected_result(), Some(&flatpak));
    let now = Instant::now();
    press(&mut app, "i", now);
    assert!(matches!(
        &app.confirmation,
        Some(Transaction::Flatpak(crate::flatpak::Operation::Install(result)))
            if result.remote == "flathub"
                && result.reference == "app/org.example.Demo/x86_64/stable"
                && result.installation == crate::flatpak::Installation::User
    ));
    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    press(&mut app, "ji", now);
    assert!(matches!(
        &app.confirmation,
        Some(Transaction::AppImage(crate::appimage::Operation::Import(url)))
            if url == "https://example.org/demo.AppImage"
    ));
}

#[test]
fn apps_tab_is_reachable_without_enabling_package_sources() {
    let mut config = Config::default();
    config.packages.flatpak_enabled = false;
    config.packages.appimage_enabled = false;
    let mut app = App::new(config, None, None);
    let now = Instant::now();

    app.handle_key(KeyEvent::from(KeyCode::Tab), now);
    app.handle_key(KeyEvent::from(KeyCode::Tab), now);

    assert_eq!(app.view.title(), "Apps");
    assert!(app.views().any(|view| view.title() == "Apps"));
    assert!(app.has_pending_reads());

    press(&mut app, "diuUxX u", now);
    assert!(app.confirmation.is_none());
    assert!(app.take_transaction().is_none());
}

#[test]
fn apps_filter_uses_the_launcher_name_and_restores_the_list_on_escape() {
    let mut app = App {
        view: View::Apps,
        ..App::default()
    };
    let entry = |id: &str, name: &str| crate::desktop_apps::Entry {
        id: id.into(),
        name: name.into(),
        comment: String::new(),
        path: PathBuf::from(format!("/examples/{id}.desktop")),
    };
    assert!(app.apply_read(Response::DesktopApps(
        0,
        Ok(vec![
            entry("org.example.Browser", "Web Browser"),
            entry("org.example.Editor", "Editor"),
        ]),
    )));
    assert_eq!(app.desktop_app_rows().count(), 2);

    let now = Instant::now();
    press(&mut app, "/BROWSER", now);
    assert_eq!(app.selected_desktop_app().unwrap().name, "Web Browser");
    assert_eq!(app.desktop_app_rows().count(), 1);

    app.handle_key(KeyEvent::from(KeyCode::Esc), now);
    assert_eq!(app.desktop_app_rows().count(), 2);
}

#[test]
fn legacy_enter_repeats_stay_suppressed_after_a_launch_finishes() {
    let mut app = App {
        view: View::Apps,
        ..App::default()
    };
    app.apply_read(Response::DesktopApps(
        0,
        Ok(vec![crate::desktop_apps::Entry {
            id: "fixture".into(),
            name: "Fixture".into(),
            comment: String::new(),
            path: PathBuf::from("/nonexistent/archoxide-test.desktop"),
        }]),
    ));

    let start = Instant::now();
    let enter = KeyEvent::from(KeyCode::Enter);
    app.handle_key(enter, start);
    assert!(app.desktop_apps.launching);
    app.apply_read(Response::LaunchDesktopApp("Fixture".into(), Ok(())));

    for elapsed in [660, 700, 740, 780, 820, 860, 900] {
        app.handle_key(enter, start + Duration::from_millis(elapsed));
        assert!(
            !app.desktop_apps.launching,
            "repeat at {elapsed}ms relaunched"
        );
    }

    app.handle_key(enter, start + Duration::from_millis(1800));
    assert!(app.desktop_apps.launching);
}
