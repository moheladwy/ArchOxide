use std::{
    collections::HashSet,
    env, fmt, io,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output, Stdio},
};

use crate::{appimage, flatpak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Repository,
    Foreign,
    Unknown,
}

impl Availability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Repository => "Repository",
            Self::Foreign => "Foreign",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Repository(String),
    Aur,
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(name) => formatter.write_str(name),
            Self::Aur => formatter.write_str("AUR"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub name: String,
    pub version: String,
    pub source: Source,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchSource {
    Repository,
    Aur,
}

#[derive(Debug)]
pub struct Inventory {
    pub packages: Vec<InstalledPackage>,
    pub warning: Option<String>,
}

pub fn installed() -> Result<Inventory, String> {
    let output = read("pacman", &["-Q"])?;
    let rows = pacman_stdout(&output, "pacman -Q", false)?;
    let mut packages = parse_installed(rows)?;

    let foreign = read("pacman", &["-Qqm"]).and_then(|output| {
        let names = pacman_stdout(&output, "pacman -Qqm", true)?;
        parse_package_names(names, "pacman -Qqm")
    });
    let warning = match foreign {
        Ok(names) => {
            for package in &mut packages {
                package.availability = if names.contains(&package.name) {
                    Availability::Foreign
                } else {
                    Availability::Repository
                };
            }
            None
        }
        Err(error) => Some(error),
    };
    Ok(Inventory { packages, warning })
}

pub fn search(source: SearchSource, query: &str) -> Result<Vec<SearchResult>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if query.chars().any(char::is_control) {
        return Err("Search terms cannot contain control characters".into());
    }

    let output = match source {
        SearchSource::Repository => {
            let pattern = literal_pattern(query);
            read("pacman", &["-Ss", "--color", "never", "--", &pattern])?
        }
        SearchSource::Aur => read(
            "yay",
            &[
                "-Ssa",
                "--color",
                "never",
                "--doublelineresults",
                "--",
                query,
            ],
        )?,
    };

    let rows = match source {
        SearchSource::Repository => pacman_stdout(&output, "pacman -Ss", true)?,
        SearchSource::Aur => yay_search_stdout(&output)?,
    };
    parse_search(rows, source)
}

pub fn yay_available() -> bool {
    let path = env::var_os("PATH").unwrap_or_else(|| "/bin:/usr/bin".into());
    env::split_paths(&path).any(|directory| executable(&directory.join("yay")))
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|info| info.is_file() && info.permissions().mode() & 0o111 != 0)
}

#[derive(Clone, Debug)]
pub enum Transaction {
    Install(SearchResult),
    Remove(Vec<String>),
    Upgrade { aur: bool },
    UpgradeSelected { aur: bool, names: Vec<String> },
    UpgradeExcept { aur: bool, names: Vec<String> },
    Flatpak(flatpak::Operation),
    AppImage(appimage::Operation),
}

impl Transaction {
    pub fn commands(&self) -> Result<Vec<Command>, String> {
        match self {
            Self::Flatpak(operation) => operation.commands(),
            Self::Remove(names) => {
                let command = self.native_command()?;
                // Pacman can interpret a missing package name as a group.
                require_installed_targets(names, &installed_names()?)?;
                Ok(vec![command])
            }
            _ => self.native_command().map(|command| vec![command]),
        }
    }

    fn native_command(&self) -> Result<Command, String> {
        let mut command;
        match self {
            Self::Install(package) => {
                validate_package(&package.name)?;
                let repository = match &package.source {
                    Source::Repository(repository) => {
                        if !valid_repository(repository) {
                            return Err("Invalid package repository".into());
                        }
                        command = Command::new("sudo");
                        command.arg("pacman");
                        repository
                    }
                    Source::Aur => {
                        command = Command::new("yay");
                        "aur"
                    }
                };
                command.args(["-S", "--needed", "--"]);
                command.arg(format!("{repository}/{}", package.name));
            }
            Self::Remove(names) => {
                require_selection(names)?;
                command = Command::new("sudo");
                command.args(["pacman", "-R", "--"]).args(names);
            }
            Self::Upgrade { aur } => return upgrade_command(*aur, true, std::iter::empty()),
            Self::UpgradeSelected { aur, names } => {
                return selected_upgrade_command(*aur, names, &installed_names()?);
            }
            Self::UpgradeExcept { aur, names } => {
                require_selection(names)?;
                return upgrade_command(*aur, true, names.iter().map(String::as_str));
            }
            Self::Flatpak(_) | Self::AppImage(_) => {
                return Err("Expected a native package operation".into());
            }
        }
        Ok(command)
    }
}

fn require_selection(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("Select at least one installed package".into());
    }
    names.iter().try_for_each(|name| validate_package(name))
}

fn installed_names() -> Result<HashSet<String>, String> {
    let output = read("pacman", &["-Qq"])?;
    let rows = pacman_stdout(&output, "pacman -Qq", true)?;
    parse_package_names(rows, "pacman -Qq")
}

fn require_installed_targets(names: &[String], installed: &HashSet<String>) -> Result<(), String> {
    if let Some(missing) = names.iter().find(|name| !installed.contains(*name)) {
        return Err(format!(
            "{missing} is no longer installed. Refresh Installed and try again."
        ));
    }
    Ok(())
}

fn selected_upgrade_command(
    aur: bool,
    names: &[String],
    installed: &HashSet<String>,
) -> Result<Command, String> {
    require_selection(names)?;
    require_installed_targets(names, installed)?;
    let selected: HashSet<_> = names.iter().map(String::as_str).collect();
    let mut ignored: Vec<_> = installed
        .iter()
        .map(String::as_str)
        .filter(|name| !selected.contains(name))
        .collect();
    ignored.sort_unstable();
    // A single sysupgrade flag only upgrades newer versions and keeps dependency checks.
    upgrade_command(aur, false, ignored)
}

fn upgrade_command<'a>(
    aur: bool,
    refresh: bool,
    ignored: impl IntoIterator<Item = &'a str>,
) -> Result<Command, String> {
    let mut command = if aur {
        Command::new("yay")
    } else {
        let mut command = Command::new("sudo");
        command.arg("pacman");
        command
    };
    command.arg(if refresh { "-Syu" } else { "-Su" });

    // Batch comma-separated names to stay below Linux's per-argument size limit.
    let mut batch = String::new();
    for name in ignored {
        validate_package(name)?;
        if !batch.is_empty() && batch.len() + name.len() >= 16 * 1024 {
            command.args(["--ignore", &batch]);
            batch.clear();
        }
        if !batch.is_empty() {
            batch.push(',');
        }
        batch.push_str(name);
    }
    if !batch.is_empty() {
        command.args(["--ignore", &batch]);
    }
    Ok(command)
}

fn read(program: &str, args: &[&str]) -> Result<Output, String> {
    Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("LANGUAGE", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => format!("{program} was not found on PATH"),
            _ => format!("Could not start {program}: {error}"),
        })
}

fn pacman_stdout<'a>(
    output: &'a Output,
    invocation: &str,
    require_clean_stderr: bool,
) -> Result<&'a [u8], String> {
    if output.status.code() == Some(1) && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok(&[]);
    }
    if output.status.success() && (!require_clean_stderr || output.stderr.is_empty()) {
        return Ok(&output.stdout);
    }
    Err(read_error(invocation, output))
}

fn yay_search_stdout(output: &Output) -> Result<&[u8], String> {
    // yay can report an AUR RPC failure on stderr and still exit successfully.
    if output.status.success() && output.stderr.is_empty() {
        return Ok(&output.stdout);
    }
    Err(read_error("yay -Ssa", output))
}

fn read_error(invocation: &str, output: &Output) -> String {
    let summary = if output.status.success() {
        format!("{invocation} reported a diagnostic")
    } else {
        format!("{invocation} failed ({})", output.status)
    };
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    let diagnostic = display_text(&diagnostic.split_whitespace().collect::<Vec<_>>().join(" "));
    if diagnostic.is_empty() {
        summary
    } else {
        format!("{summary}: {diagnostic}")
    }
}

fn literal_pattern(query: &str) -> String {
    if query == "-" {
        // pacman reads a bare `-` from stdin even after `--`.
        return "[-]".into();
    }

    let mut pattern = String::with_capacity(query.len());
    for character in query.chars() {
        if r"\.^$|?*+()[{".contains(character) {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn parse_installed(bytes: &[u8]) -> Result<Vec<InstalledPackage>, String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split_ascii_whitespace();
            let name = columns.next().filter(|name| valid_package(name));
            let version = columns.next().filter(|version| valid_version(version));
            match (name, version, columns.next()) {
                (Some(name), Some(version), None) => Ok(InstalledPackage {
                    name: name.to_owned(),
                    version: version.to_owned(),
                    availability: Availability::Unknown,
                }),
                _ => Err("Unexpected package record from pacman -Q".to_owned()),
            }
        })
        .collect()
}

fn parse_package_names(bytes: &[u8], command: &str) -> Result<HashSet<String>, String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|name| {
            validate_package(name)
                .map_err(|_| format!("Unexpected package name from {command}"))?;
            Ok(name.to_owned())
        })
        .collect()
}

fn parse_search(bytes: &[u8], source: SearchSource) -> Result<Vec<SearchResult>, String> {
    let stdout = String::from_utf8_lossy(bytes);
    let mut lines = stdout.lines().peekable();
    let mut results = Vec::new();
    let unexpected = || match source {
        SearchSource::Repository => "Unexpected search output from pacman -Ss".to_owned(),
        SearchSource::Aur => "Unexpected search output from yay -Ssa".to_owned(),
    };

    while let Some(header) = lines.next() {
        if header.trim().is_empty() {
            continue;
        }

        let (repository, details) = header.split_once('/').ok_or_else(unexpected)?;
        let source = match source {
            SearchSource::Repository if valid_repository(repository) => {
                Source::Repository(repository.to_owned())
            }
            SearchSource::Aur if repository == "aur" => Source::Aur,
            _ => return Err(unexpected()),
        };

        let mut columns = details.split_ascii_whitespace();
        let name = columns.next().filter(|name| valid_package(name));
        let version = columns.next().filter(|version| valid_version(version));
        let (Some(name), Some(version)) = (name, version) else {
            return Err(unexpected());
        };

        let mut description = display_text(
            lines
                .next()
                .filter(|line| indented(line))
                .ok_or_else(unexpected)?,
        );
        while let Some(line) = lines.next_if(|line| indented(line)) {
            let continuation = display_text(line);
            if !description.is_empty() && !continuation.is_empty() {
                description.push(' ');
            }
            description.push_str(&continuation);
        }
        results.push(SearchResult {
            name: name.to_owned(),
            version: version.to_owned(),
            source,
            description,
        });
    }
    Ok(results)
}

fn indented(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

fn display_text(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn valid_repository(repository: &str) -> bool {
    !matches!(repository, "" | "." | "..")
        && repository == repository.trim()
        && !repository.contains('/')
        && !repository.chars().any(char::is_control)
}

fn validate_package(name: &str) -> Result<(), String> {
    if valid_package(name) {
        Ok(())
    } else {
        Err("Invalid package name".into())
    }
}

fn valid_package(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '.'])
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._+-".contains(&byte))
}

fn valid_version(version: &str) -> bool {
    !version.chars().any(char::is_control)
}

#[cfg(test)]
#[path = "../tests/unit/packages.rs"]
mod tests;
