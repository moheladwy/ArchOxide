use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub appearance: Appearance,
    pub packages: PackageSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub theme: Theme,
    pub density: Density,
    pub details_width_percent: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f64>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            density: Density::Comfortable,
            details_width_percent: 40,
            font_family: None,
            font_size_pt: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Terminal,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Terminal => "Terminal",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
}

impl Density {
    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Comfortable => "Comfortable",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PackageSettings {
    pub aur_enabled: bool,
}

impl Default for PackageSettings {
    fn default() -> Self {
        Self { aur_enabled: true }
    }
}

pub fn config_path() -> Result<PathBuf, String> {
    resolve_config_path(
        env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
    )
}

fn resolve_config_path(
    config_home: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, String> {
    let directory = config_home
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|path| path.join(".config"))
        })
        .ok_or_else(|| {
            "Set XDG_CONFIG_HOME or HOME to an absolute path to use settings".to_owned()
        })?;
    Ok(directory.join("archoxide").join("config.toml"))
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(format!(
                    "Could not read settings at {} ({error})",
                    path.display()
                ));
            }
        };
        let config: Self = toml::from_str(&contents)
            .map_err(|error| format!("Invalid settings at {} ({error})", path.display()))?;
        config
            .validate()
            .map_err(|error| format!("Invalid settings at {} ({error})", path.display()))?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let contents = toml::to_string_pretty(self)
            .map_err(|error| format!("Could not serialize settings ({error})"))?;
        let directory = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let failed =
            |error: io::Error| format!("Could not save settings to {} ({error})", path.display());
        fs::create_dir_all(directory).map_err(failed)?;
        let mut file = NamedTempFile::new_in(directory).map_err(failed)?;
        file.write_all(contents.as_bytes()).map_err(failed)?;
        file.as_file().sync_all().map_err(failed)?;
        file.persist(path).map_err(|error| failed(error.error))?;
        Ok(())
    }

    pub fn launch_kitty(&self) -> Result<(), String> {
        self.validate()?;
        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the ArchOxide executable ({error})"))?;
        let status = self.kitty_command(&executable).status().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "Kitty executable was not found in PATH".to_owned()
            } else {
                format!("Could not launch Kitty ({error})")
            }
        })?;
        if !status.success() {
            return Err(format!("Kitty exited with {status}"));
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if !(25..=60).contains(&self.appearance.details_width_percent) {
            return Err("Details pane width must be between 25% and 60%".to_owned());
        }
        if let Some(family) = &self.appearance.font_family
            && (family.trim().is_empty()
                || family.chars().any(|character| {
                    character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
                }))
        {
            return Err("Font family must contain a printable, single-line name".to_owned());
        }
        if let Some(size) = self.appearance.font_size_pt
            && !(6.0..=48.0).contains(&size)
        {
            return Err("Font size must be between 6 and 48 points".to_owned());
        }
        Ok(())
    }

    fn kitty_command(&self, executable: &Path) -> Command {
        let mut command = Command::new("kitty");
        // A Kitty startup session would ignore the executable passed on the command line.
        command.args(["--session", "none"]);
        if let Some(family) = &self.appearance.font_family {
            command.args(["--override", &format!("font_family={}", family.trim())]);
        }
        if let Some(size) = self.appearance.font_size_pt {
            command.args(["--override", &format!("font_size={size}")]);
        }
        command.arg("--").arg(executable);
        command
    }
}

#[cfg(test)]
mod tests {
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
            packages: PackageSettings { aur_enabled: false },
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
}
