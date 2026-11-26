use std::{
    collections::HashSet,
    env, fmt, io,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output, Stdio},
};

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
    Remove(InstalledPackage),
    Upgrade { aur: bool },
    UpgradeSelected { aur: bool, names: Vec<String> },
    UpgradeExcept { aur: bool, names: Vec<String> },
}

impl Transaction {
    pub fn command(&self) -> Result<Command, String> {
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
            Self::Remove(package) => {
                validate_package(&package.name)?;
                command = Command::new("sudo");
                command.args(["pacman", "-R", "--", &package.name]);
            }
            Self::Upgrade { aur } => return upgrade_command(*aur, true, std::iter::empty()),
            Self::UpgradeSelected { aur, names } => {
                let output = read("pacman", &["-Qq"])?;
                let rows = pacman_stdout(&output, "pacman -Qq", true)?;
                let installed = parse_package_names(rows, "pacman -Qq")?;
                return selected_upgrade_command(*aur, names, &installed);
            }
            Self::UpgradeExcept { aur, names } => {
                require_selection(names)?;
                return upgrade_command(*aur, true, names.iter().map(String::as_str));
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

fn selected_upgrade_command(
    aur: bool,
    names: &[String],
    installed: &HashSet<String>,
) -> Result<Command, String> {
    require_selection(names)?;
    if let Some(missing) = names.iter().find(|name| !installed.contains(*name)) {
        return Err(format!(
            "{missing} is no longer installed. Refresh Installed and try again."
        ));
    }
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
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    #[test]
    fn selected_upgrade_ignores_other_installed_packages_without_refreshing() {
        let installed = ["alpha", "beta", "gamma"]
            .map(str::to_owned)
            .into_iter()
            .collect();
        let names = vec!["beta".to_owned()];
        for aur in [false, true] {
            let command = selected_upgrade_command(aur, &names, &installed).unwrap();
            let args: Vec<_> = command.get_args().collect();
            if aur {
                assert_eq!(command.get_program(), "yay");
                assert_eq!(args, ["-Su", "--ignore", "alpha,gamma"]);
            } else {
                assert_eq!(command.get_program(), "sudo");
                assert_eq!(args, ["pacman", "-Su", "--ignore", "alpha,gamma"]);
            }
        }
        assert!(selected_upgrade_command(false, &[], &installed).is_err());
        assert!(selected_upgrade_command(false, &["missing".into()], &installed).is_err());
    }

    #[test]
    fn update_except_refreshes_with_validated_temporary_exclusions() {
        for aur in [false, true] {
            let command = Transaction::UpgradeExcept {
                aur,
                names: vec!["alpha".into(), "beta".into()],
            }
            .command()
            .unwrap();
            let args: Vec<_> = command.get_args().collect();
            let expected = if aur {
                &["-Syu", "--ignore", "alpha,beta"][..]
            } else {
                &["pacman", "-Syu", "--ignore", "alpha,beta"][..]
            };
            assert_eq!(args, expected);
        }
        for names in [vec![], vec!["alpha,beta".into()], vec!["--nodeps".into()]] {
            assert!(
                Transaction::UpgradeExcept { aur: false, names }
                    .command()
                    .is_err()
            );
        }
    }

    fn command_output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn parses_installed_versions_and_foreign_names() {
        let installed = parse_installed(b"alsa-lib 1.2.15.3-1\nyay 2:13.1.0-1\n").unwrap();
        assert_eq!(installed[0].name, "alsa-lib");
        assert_eq!(installed[1].version, "2:13.1.0-1");
        assert_eq!(installed[1].availability, Availability::Unknown);
        assert_eq!(
            parse_package_names(b"yay\n", "pacman -Qqm").unwrap(),
            HashSet::from(["yay".into()])
        );
        assert!(parse_package_names(b"", "pacman -Qqm").unwrap().is_empty());
    }

    #[test]
    fn parses_package_identity_separately_for_each_source() {
        let repository = parse_search(
            b"core/demo 2:1.4-1 (base) [installed: 2:1.3-1]\n    An example\n\
              extra/demo 1.5-1\n    Another example\n",
            SearchSource::Repository,
        )
        .unwrap();
        let aur = parse_search(
            b"aur/demo 1.7-2 (+40 0.43) [9d4h] (Out-of-date: 2026-11-26) \n    A third example\n",
            SearchSource::Aur,
        )
        .unwrap();
        assert_eq!(repository.len(), 2);
        assert_eq!(repository[0].source, Source::Repository("core".into()));
        assert_eq!(repository[0].name, "demo");
        assert_eq!(repository[0].version, "2:1.4-1");
        assert_eq!(repository[0].description, "An example");
        assert_eq!(repository[1].source, Source::Repository("extra".into()));
        assert_eq!(repository[1].name, "demo");
        assert_eq!(aur[0].source, Source::Aur);
        assert_eq!(aur[0].name, "demo");
        assert_eq!(aur[0].version, "1.7-2");
    }

    #[test]
    fn contains_description_control_characters_and_rejects_malformed_records() {
        let rows = parse_search(
            "aur/demo 1.0-1 (+4 0.00) \n    A\u{1b}[31m demo\u{7}\u{9b}\n    continued\n"
                .as_bytes(),
            SearchSource::Aur,
        )
        .unwrap();
        assert_eq!(rows[0].description, "A[31m demo continued");
        assert!(!rows[0].description.chars().any(char::is_control));
        assert!(parse_search(b"aur/demo 1.0-1\n", SearchSource::Aur).is_err());
        assert!(parse_search(b"extra/demo 1.0-1\n    Description\n", SearchSource::Aur).is_err());
    }

    #[test]
    fn distinguishes_empty_pacman_results_from_diagnostics_and_failure() {
        let no_match = command_output(1, b"", b"");
        assert!(
            pacman_stdout(&no_match, "pacman -Q", false)
                .unwrap()
                .is_empty()
        );
        assert!(
            pacman_stdout(&no_match, "pacman -Qqm", true)
                .unwrap()
                .is_empty()
        );
        assert!(
            pacman_stdout(&no_match, "pacman -Ss", true)
                .unwrap()
                .is_empty()
        );

        let error = command_output(
            1,
            b"",
            b"error: no usable package repositories configured.\n",
        );
        assert!(
            pacman_stdout(&error, "pacman -Ss", true)
                .unwrap_err()
                .contains("no usable package repositories")
        );
        assert!(pacman_stdout(&command_output(1, b"yay\n", b""), "pacman -Qqm", true).is_err());
        let partial = command_output(
            0,
            b"demo 1.0-1\n",
            b"warning: database file does not exist\n",
        );
        assert!(pacman_stdout(&partial, "pacman -Qqm", true).is_err());
        assert_eq!(
            pacman_stdout(&partial, "pacman -Q", false).unwrap(),
            b"demo 1.0-1\n"
        );
    }

    #[test]
    fn recognizes_yay_rpc_diagnostics_even_with_zero_exit_status() {
        assert!(
            yay_search_stdout(&command_output(0, b"", b""))
                .unwrap()
                .is_empty()
        );
        assert!(yay_search_stdout(&command_output(1, b"", b"")).is_err());
        let error = command_output(
            0,
            b"",
            b" -> Error during AUR search: 1 error occurred:\n\t* request failed\n",
        );
        let message = yay_search_stdout(&error).unwrap_err();
        assert!(message.contains("Error during AUR search"));
        assert!(message.contains("request failed"));
        assert!(!message.chars().any(char::is_control));
    }

    #[test]
    fn uses_literal_posix_search_terms_and_skips_empty_queries() {
        assert_eq!(
            literal_pattern(r"[a].+(b)?{2}|^$\"),
            r"\[a]\.\+\(b\)\?\{2}\|\^\$\\"
        );
        assert_eq!(literal_pattern("-"), "[-]");
        assert_eq!(literal_pattern("--color never"), "--color never");
        assert!(search(SearchSource::Repository, " \t ").unwrap().is_empty());
        assert!(search(SearchSource::Aur, "text\0text").is_err());
    }

    #[test]
    fn builds_interactive_transactions_with_exact_arguments() {
        let sample = SearchResult {
            name: "demo-name+@_2".into(),
            version: "3.0-1".into(),
            source: Source::Repository("extra".into()),
            description: String::new(),
        };
        let cases: [(Transaction, &str, &[&str]); 5] = [
            (
                Transaction::Install(sample.clone()),
                "sudo",
                &["pacman", "-S", "--needed", "--", "extra/demo-name+@_2"],
            ),
            (
                Transaction::Install(SearchResult {
                    source: Source::Aur,
                    ..sample
                }),
                "yay",
                &["-S", "--needed", "--", "aur/demo-name+@_2"],
            ),
            (
                Transaction::Remove(InstalledPackage {
                    name: "demo-name+@_2".into(),
                    version: "3.0-1".into(),
                    availability: Availability::Foreign,
                }),
                "sudo",
                &["pacman", "-R", "--", "demo-name+@_2"],
            ),
            (Transaction::Upgrade { aur: true }, "yay", &["-Syu"]),
            (
                Transaction::Upgrade { aur: false },
                "sudo",
                &["pacman", "-Syu"],
            ),
        ];

        for (transaction, program, args) in cases {
            let command = transaction.command().unwrap();
            assert_eq!(command.get_program(), program);
            assert!(command.get_args().eq(args.iter().copied()));
            assert_eq!(command.get_envs().count(), 0);
        }
    }

    #[test]
    fn rejects_invalid_transaction_targets() {
        for name in ["", "-Syu", ".hidden", "core/demo", "demo\tname", "demo\n"] {
            let package = InstalledPackage {
                name: name.into(),
                version: "1.0-1".into(),
                availability: Availability::Unknown,
            };
            assert!(Transaction::Remove(package).command().is_err());
        }
        for repository in ["", "..", "extra/demo", "extra\x1b"] {
            let package = SearchResult {
                name: "demo".into(),
                version: "1.0-1".into(),
                source: Source::Repository(repository.into()),
                description: String::new(),
            };
            assert!(Transaction::Install(package).command().is_err());
        }
    }
}
