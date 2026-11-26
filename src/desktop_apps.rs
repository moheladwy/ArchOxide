use std::{
    collections::HashSet,
    env, io,
    os::{
        fd::OwnedFd,
        unix::{net::UnixDatagram, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use freedesktop_desktop_entry::{DesktopEntry, Iter, get_languages_from_env};

use crate::packages::executable;

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub path: PathBuf,
}

impl Entry {
    pub fn launch(&self) -> Result<(), String> {
        // An app may inherit gio's stderr. A nonblocking datagram socket lets us
        // collect the diagnostic without waiting for the app to close its stderr.
        let (diagnostics, stderr) = UnixDatagram::pair()
            .and_then(|(receiver, sender)| {
                receiver.set_nonblocking(true)?;
                sender.set_nonblocking(true)?;
                Ok((receiver, sender))
            })
            .map_err(|error| format!("Could not capture gio diagnostics: {error}"))?;
        let mut command = Command::new("gio");
        command
            .arg("launch")
            .arg(&self.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(OwnedFd::from(stderr));
        // SAFETY: The child callback uses only setsid and reads errno on failure.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let status = command.status().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "Opening desktop apps requires gio, provided by glib2 on Arch Linux.".into()
            } else {
                format!("Could not open {}: {error}", self.name)
            }
        })?;
        if !status.success() {
            let mut buffer = [0; 4096];
            let mut size = 0;
            while size < buffer.len() {
                match diagnostics.recv(&mut buffer[size..]) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => size += read,
                }
            }
            let diagnostic = String::from_utf8_lossy(&buffer[..size]);
            let detail = diagnostic.trim();
            return Err(if detail.is_empty() {
                format!(
                    "Could not open {}: gio launch returned {status}.",
                    self.name
                )
            } else {
                format!("Could not open {}: {detail}", self.name)
            });
        }
        Ok(())
    }
}

pub fn installed() -> Vec<Entry> {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        });
    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let application_dirs = data_home
        .into_iter()
        .chain(env::split_paths(&data_dirs).filter(|path| path.is_absolute()))
        .map(|path| path.join("applications"));
    let session = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktops: Vec<_> = session.split(':').filter(|name| !name.is_empty()).collect();
    discover(application_dirs, &get_languages_from_env(), &desktops)
}

fn discover(
    application_dirs: impl Iterator<Item = PathBuf>,
    locales: &[String],
    desktops: &[&str],
) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for entry in Iter::new(application_dirs).entries(Some(locales)) {
        // The first file for an ID wins, even when it hides the system launcher.
        if !seen.insert(entry.id().to_owned())
            || entry.hidden()
            || entry.no_display()
            || entry.type_() != Some("Application")
            || (!entry.dbus_activatable() && entry.exec().is_none_or(|exec| exec.trim().is_empty()))
            || !visible_on_desktop(&entry, desktops)
            || entry
                .try_exec()
                .is_some_and(|command| !command_available(command))
        {
            continue;
        }
        let Some(name) = entry.name(locales).filter(|name| !name.trim().is_empty()) else {
            continue;
        };
        apps.push(Entry {
            id: entry.id().to_owned(),
            name: name.into_owned(),
            comment: entry.comment(locales).unwrap_or_default().into_owned(),
            path: entry.path,
        });
    }
    apps.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.clone()));
    apps
}

fn visible_on_desktop(entry: &DesktopEntry, desktops: &[&str]) -> bool {
    let included = entry.only_show_in();
    let excluded = entry.not_show_in().unwrap_or_default();
    for desktop in desktops {
        if included.as_ref().is_some_and(|list| list.contains(desktop)) {
            return true;
        }
        if excluded.contains(desktop) {
            return false;
        }
    }
    included.is_none()
}

fn command_available(command: &str) -> bool {
    let command = Path::new(command);
    if command.is_absolute() {
        return executable(command);
    }
    let path = env::var_os("PATH").unwrap_or_else(|| "/bin:/usr/bin".into());
    env::split_paths(&path).any(|directory| executable(&directory.join(command)))
}

#[cfg(test)]
#[path = "../tests/unit/desktop_apps.rs"]
mod tests;
