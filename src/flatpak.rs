use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt, fs, io,
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Installation {
    User,
    System,
    Named(String),
}

impl Installation {
    fn parse(label: &str) -> Result<Self, String> {
        match label {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            _ => label
                .strip_prefix("system (")
                .and_then(|name| name.strip_suffix(')'))
                .filter(|name| valid_installation_name(name))
                .map(|name| Self::Named(name.into()))
                .ok_or_else(|| "Flatpak returned an invalid installation name".into()),
        }
    }

    fn command(&self, action: &str) -> Command {
        let mut command = Command::new("flatpak");
        command.arg(action);
        match self {
            Self::User => command.arg("--user"),
            Self::System => command.arg("--system"),
            Self::Named(name) => command.arg(format!("--installation={name}")),
        };
        command
    }
}

impl fmt::Display for Installation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => formatter.write_str("User"),
            Self::System => formatter.write_str("System"),
            Self::Named(name) => write!(formatter, "System ({name})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub reference: String,
    pub remote: String,
    pub installation: Installation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    pub reference: String,
    pub description: String,
    pub remote: String,
    pub installation: Installation,
}

pub fn installed() -> Result<Vec<InstalledApp>, String> {
    let output = read(Command::new("flatpak").args([
        "list",
        "--app",
        "--columns=ref,installation,origin,version,name",
    ]))?;
    parse_installed(&output)
}

pub fn info(
    installation: &Installation,
    remote: Option<&str>,
    reference: &str,
) -> Result<String, String> {
    app_identity(reference)?;
    if let Some(remote) = remote {
        validate_remote(remote)?;
        read(
            installation
                .command("remote-info")
                .args(["--", remote, reference]),
        )
    } else {
        read(installation.command("info").args(["--", reference]))
    }
}

pub fn search(query: &str) -> Result<Vec<SearchApp>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if query.chars().any(char::is_control) {
        return Err("Search terms cannot contain control characters".into());
    }

    let query = query.to_lowercase();
    let mut matches = Vec::new();
    let mut errors = Vec::new();
    for installation in search_installations()? {
        match search_remotes(&installation) {
            Ok(remotes) => {
                for remote in remotes {
                    match search_remote(&installation, remote, &query) {
                        Ok(apps) => matches.extend(apps),
                        Err(error) => errors.push(error),
                    }
                }
            }
            Err(error) => errors.push(error),
        }
    }
    if matches.is_empty() && !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(matches)
}

fn search_installations() -> Result<Vec<Installation>, String> {
    let mut installations = vec![Installation::User, Installation::System];
    // The CLI lists named installation paths but cannot list their IDs directly.
    let config_root = env::var_os("FLATPAK_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/flatpak"));
    let config_dir = config_root.join("installations.d");
    let entries = match fs::read_dir(&config_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(installations),
        Err(error) => return Err(format!("Cannot read {}: {error}", config_dir.display())),
    };

    let mut names = BTreeSet::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Cannot read {}: {error}", config_dir.display()))?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "conf")
            && path.is_file()
        {
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
            names.extend(
                contents
                    .lines()
                    .filter_map(installation_name)
                    .map(String::from),
            );
        }
    }
    installations.extend(names.into_iter().map(Installation::Named));
    Ok(installations)
}

fn installation_name(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("[Installation \"")
        .and_then(|line| line.strip_suffix("\"]"))
        .filter(|name| valid_installation_name(name))
}

fn valid_installation_name(name: &str) -> bool {
    !matches!(name, "" | "default" | "system" | "user")
        && name.len() <= 80
        && name.is_ascii()
        && !name.contains([' ', '/'])
        && !name.chars().any(char::is_control)
}

fn search_remotes(installation: &Installation) -> Result<Vec<String>, String> {
    let output = read(
        installation
            .command("remotes")
            .arg("--columns=name,options"),
    )?;
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split_once('\t').unwrap_or((line, "")))
        .filter(|(_, options)| {
            !options
                .split(',')
                .map(str::trim)
                .any(|option| option == "no-enumerate")
        })
        .map(|(name, _)| {
            validate_remote(name)?;
            Ok(name.into())
        })
        .collect()
}

fn search_remote(
    installation: &Installation,
    remote: String,
    query: &str,
) -> Result<Vec<SearchApp>, String> {
    // Query each scope separately so remotes sharing a name remain distinct.
    let read_remote = |cached| {
        let mut command = installation.command("remote-ls");
        if cached {
            command.arg("--cached");
        }
        read(command.args([
            "--app",
            "--columns=ref,version,name,description",
            "--",
            &remote,
        ]))
    };
    let output = read_remote(true)
        .or_else(|_| read_remote(false))
        .map_err(|error| format!("{installation} remote {remote}: {error}"))?;
    parse_search(&output, installation, &remote, query)
}

fn parse_installed(output: &str) -> Result<Vec<InstalledApp>, String> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.splitn(5, '\t');
            let reference = fields.next().unwrap_or_default();
            // Unlike remote-ls, list emits a ref without its app/runtime prefix.
            let reference = format!("app/{reference}");
            let (id, _) = app_identity(&reference)?;
            let installation = Installation::parse(fields.next().unwrap_or_default())?;
            let remote = fields.next().unwrap_or_default().into();
            let version = display_text(fields.next().unwrap_or_default());
            let name = display_text(fields.next().unwrap_or(id));
            Ok(InstalledApp {
                id: id.into(),
                name,
                version,
                reference,
                remote,
                installation,
            })
        })
        .collect()
}

fn parse_search(
    output: &str,
    installation: &Installation,
    remote: &str,
    query: &str,
) -> Result<Vec<SearchApp>, String> {
    let mut matches = Vec::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let mut fields = line.splitn(4, '\t');
        let reference = fields.next().unwrap_or_default();
        let (id, branch) = app_identity(reference)?;
        let version = display_text(fields.next().unwrap_or_default());
        let name = display_text(fields.next().unwrap_or(id));
        let description = display_text(fields.next().unwrap_or_default());
        if [id, &name, &description]
            .iter()
            .any(|field| field.to_lowercase().contains(query))
        {
            matches.push(SearchApp {
                id: id.into(),
                name,
                version,
                branch: branch.into(),
                reference: reference.into(),
                description,
                remote: remote.into(),
                installation: installation.clone(),
            });
        }
    }
    Ok(matches)
}

fn display_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

pub(crate) fn app_identity(reference: &str) -> Result<(&str, &str), String> {
    let mut components = reference.split('/');
    match (
        components.next(),
        components.next(),
        components.next(),
        components.next(),
        components.next(),
    ) {
        (Some("app"), Some(id), Some(arch), Some(branch), None)
            if [id, arch, branch].into_iter().all(valid_ref_part) =>
        {
            Ok((id, branch))
        }
        _ => Err("Flatpak returned an invalid application ref".into()),
    }
}

fn valid_ref_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_remote(remote: &str) -> Result<(), String> {
    if valid_ref_part(remote) && !remote.starts_with(['-', '.']) {
        Ok(())
    } else {
        Err("Flatpak returned an invalid remote name".into())
    }
}

fn read(command: &mut Command) -> Result<String, String> {
    let action = command
        .get_args()
        .next()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let output = command
        .env("LC_ALL", "C.UTF-8")
        .env_remove("FLATPAK_FANCY_OUTPUT")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => {
                "Flatpak is unavailable. Install the flatpak package to use this source.".into()
            }
            _ => format!("Cannot run flatpak {action}: {error}"),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().rfind(|line| !line.trim().is_empty());
        return Err(match detail {
            Some(detail) => format!("flatpak {action}: {}", display_text(detail)),
            None => format!("flatpak {action} exited with {}", output.status),
        });
    }
    String::from_utf8(output.stdout).map_err(|_| format!("flatpak {action} returned invalid UTF-8"))
}

#[derive(Clone, Debug)]
pub enum Operation {
    Install(SearchApp),
    Remove(Vec<InstalledApp>),
    UpdateAll,
    Update(Vec<InstalledApp>),
    UpdateExcept(Vec<InstalledApp>),
}

impl Operation {
    pub fn commands(&self) -> Result<Vec<Command>, String> {
        match self {
            Self::Install(app) => {
                validate_remote(&app.remote)?;
                app_identity(&app.reference)?;
                let mut command = app.installation.command("install");
                command.args(["--", &app.remote, &app.reference]);
                Ok(vec![command])
            }
            Self::Remove(apps) => {
                require_selection(apps)?;
                installed_app_commands("uninstall", &["--no-related"], apps.iter())
            }
            Self::UpdateAll => {
                let mut command = Command::new("flatpak");
                command.arg("update");
                Ok(vec![command])
            }
            Self::Update(apps) => {
                require_selection(apps)?;
                installed_app_commands("update", &[], apps.iter())
            }
            Self::UpdateExcept(excluded) => {
                require_selection(excluded)?;
                let installed = installed()?;
                update_except_commands(excluded, &installed)
            }
        }
    }
}

fn require_selection(apps: &[InstalledApp]) -> Result<(), String> {
    if apps.is_empty() {
        return Err("Select at least one installed Flatpak application".into());
    }
    Ok(())
}

fn update_except_commands(
    excluded: &[InstalledApp],
    installed: &[InstalledApp],
) -> Result<Vec<Command>, String> {
    let excluded: BTreeSet<_> = excluded
        .iter()
        .map(|app| (&app.installation, app.reference.as_str()))
        .collect();
    installed_app_commands(
        "update",
        &[],
        installed
            .iter()
            .filter(|app| !excluded.contains(&(&app.installation, app.reference.as_str()))),
    )
}

fn installed_app_commands<'a>(
    action: &str,
    options: &[&str],
    apps: impl Iterator<Item = &'a InstalledApp>,
) -> Result<Vec<Command>, String> {
    let mut grouped_refs: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for app in apps {
        app_identity(&app.reference)?;
        grouped_refs
            .entry(&app.installation)
            .or_default()
            .insert(&app.reference);
    }
    Ok(grouped_refs
        .into_iter()
        .map(|(installation, refs)| {
            let mut command = installation.command(action);
            command.args(options).arg("--").args(refs);
            command
        })
        .collect())
}

#[cfg(test)]
#[path = "../tests/unit/flatpak.rs"]
mod tests;
