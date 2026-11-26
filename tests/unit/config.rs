use super::*;

#[test]
fn respects_absolute_xdg_config_home_and_falls_back_to_home() {
    let home = Some(PathBuf::from("/home/user"));
    assert_eq!(
        resolve_config_path(Some(PathBuf::from("/custom")), home.clone()).unwrap(),
        Path::new("/custom/archoxide/config.toml")
    );
    for config_home in [None, Some(PathBuf::new()), Some(PathBuf::from("relative"))] {
        assert_eq!(
            resolve_config_path(config_home, home.clone()).unwrap(),
            Path::new("/home/user/.config/archoxide/config.toml")
        );
    }
    assert!(resolve_config_path(None, None).is_err());
}

#[test]
fn missing_file_uses_defaults_without_creating_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let config = Config::load(&path).unwrap();
    assert_eq!(config.appearance.theme, Theme::Dark);
    assert_eq!(config.appearance.density, Density::Comfortable);
    assert_eq!(config.appearance.details_width_percent, 40);
    assert_eq!(config.appearance.font_family, None);
    assert_eq!(config.appearance.font_size_pt, None);
    assert!(config.packages.aur_enabled);
    assert!(config.packages.flatpak_enabled);
    assert!(config.packages.appimage_enabled);
    assert!(!path.exists());
}

#[test]
fn partial_toml_supplies_defaults_for_omissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    fs::write(&path, "[appearance]\ntheme = 'light'\nfont_size_pt = 12\n").unwrap();
    let expected = Config {
        appearance: Appearance {
            theme: Theme::Light,
            font_size_pt: Some(12.0),
            ..Appearance::default()
        },
        ..Config::default()
    };
    assert_eq!(Config::load(&path).unwrap(), expected);
}

#[test]
fn malformed_settings_report_an_error_and_preserve_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    for input in [
        "[appearance",
        "[appearance]\ntheme = 'unrecognized'",
        "[appearance]\ndensity = 'dense'",
        "[appearance]\ndetails_width_percent = 24",
        "[appearance]\ndetails_width_percent = 61",
        "[appearance]\nfont_family = ' '",
        "[appearance]\nfont_family = \"Family\\nName\"",
        "[appearance]\nfont_size_pt = 5.9",
        "[appearance]\nfont_size_pt = 48.1",
        "[appearance]\nfont_size_pt = nan",
        "[appearance]\nfont_size_pt = inf",
        "[packages]\naur_enabled = 'true'",
    ] {
        fs::write(&path, input).unwrap();
        let error = Config::load(&path).unwrap_err();
        assert!(error.contains("Invalid settings"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), input);
    }
}

#[test]
fn saved_settings_round_trip_and_cleared_fonts_inherit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("archoxide/config.toml");
    let config = Config {
        appearance: Appearance {
            theme: Theme::Terminal,
            density: Density::Compact,
            details_width_percent: 60,
            font_family: Some("Iosevka Term".to_owned()),
            font_size_pt: Some(6.0),
        },
        packages: PackageSettings {
            aur_enabled: false,
            flatpak_enabled: false,
            appimage_enabled: false,
        },
    };
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), config);

    let defaults = Config::default();
    defaults.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), defaults);
    let contents = fs::read_to_string(&path).unwrap();
    assert!(!contents.contains("font_family"));
    assert!(!contents.contains("font_size_pt"));
}

#[test]
fn invalid_save_preserves_the_previous_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let original = "# Keep this comment\n[packages]\naur_enabled = false\n";
    fs::write(&path, original).unwrap();
    let mut config = Config::default();
    config.appearance.font_size_pt = Some(f64::NAN);
    assert!(config.save(&path).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn failed_replacement_preserves_the_destination_and_removes_the_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    fs::create_dir(&path).unwrap();
    let existing = path.join("keep");
    fs::write(&existing, "keep").unwrap();

    assert!(Config::default().save(&path).is_err());
    assert_eq!(fs::read_to_string(&existing).unwrap(), "keep");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn kitty_passes_each_override_and_executable_as_one_argument() {
    let mut config = Config::default();
    let executable = Path::new("/a path/archoxide");
    let command = config.kitty_command(executable);
    assert_eq!(command.get_program(), "kitty");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        ["--session", "none", "--", "/a path/archoxide"]
    );

    config.appearance.font_family = Some("Iosevka Term".to_owned());
    config.appearance.font_size_pt = Some(13.5);
    assert_eq!(
        config
            .kitty_command(executable)
            .get_args()
            .collect::<Vec<_>>(),
        [
            "--session",
            "none",
            "--override",
            "font_family=Iosevka Term",
            "--override",
            "font_size=13.5",
            "--",
            "/a path/archoxide",
        ]
    );
}
