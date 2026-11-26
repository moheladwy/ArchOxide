use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::fonts;
pub use crate::themes::Theme;

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
    pub flatpak_enabled: bool,
    pub appimage_enabled: bool,
}

impl Default for PackageSettings {
    fn default() -> Self {
        Self {
            aur_enabled: true,
            flatpak_enabled: true,
            appimage_enabled: true,
        }
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
            && !fonts::is_valid_family(family)
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
#[path = "../tests/unit/config.rs"]
mod tests;
