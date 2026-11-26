use std::path::{Path, PathBuf};

use crate::{
    flatpak,
    packages::{self, InstalledPackage, SearchResult, Source},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Installed(InstalledPackage),
    Search(SearchResult),
    FlatpakInstalled(flatpak::InstalledApp),
    FlatpakRemote(flatpak::SearchApp),
    AppImage(PathBuf),
}

impl Target {
    pub fn read(&self) -> Result<String, String> {
        match self {
            Self::Installed(package) => {
                packages::read_info("pacman", &["-Qi", "--", &package.name])
            }
            Self::Search(package) => match &package.source {
                Source::Repository(repository) => {
                    let target = format!("{repository}/{}", package.name);
                    packages::read_info("pacman", &["-Si", "--", &target])
                }
                Source::Aur => packages::read_info("yay", &["-Si", "--aur", "--", &package.name]),
                Source::Flatpak {
                    installation,
                    remote,
                    reference,
                } => flatpak::info(installation, Some(remote), reference),
                Source::ManagedAppImage(path) => file_info(path),
                Source::AppImageImport(input) if Path::new(input).is_absolute() => {
                    file_info(Path::new(input))
                }
                Source::AppImageImport(_) => Ok(String::new()),
            },
            Self::FlatpakInstalled(app) => flatpak::info(&app.installation, None, &app.reference),
            Self::FlatpakRemote(app) => {
                flatpak::info(&app.installation, Some(&app.remote), &app.reference)
            }
            Self::AppImage(path) => file_info(path),
        }
    }
}

fn file_info(path: &Path) -> Result<String, String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(format!("File size: {} bytes", metadata.len()))
}
