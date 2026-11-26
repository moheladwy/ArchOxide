use std::{
    env,
    ffi::{CString, OsString},
    fs::{self, DirBuilder, File, Metadata, OpenOptions},
    io::{self, Read, Seek, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;

use crate::packages::{SearchResult, Source as PackageSource};

const EXTENSION: &str = ".AppImage";
const MANIFEST: &str = "origin.toml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub origin: Option<String>,
    directory_id: (u64, u64),
    image_id: (u64, u64),
}

#[derive(Clone, Debug)]
pub enum Operation {
    Import(String),
    Remove(Vec<Entry>),
    Update(Vec<Entry>),
    Replace(Entry, String),
}

pub fn installed() -> Result<Vec<Entry>, String> {
    let Some(store) = Store::open(false)? else {
        return Ok(Vec::new());
    };
    store.installed()
}

// AppImages have no configured remote catalog. Search managed entries and
// recognize explicit local files or HTTPS sources without downloading anything.
pub fn search(query: &str) -> Result<Vec<SearchResult>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if has_control(query) {
        return Err("Search terms cannot contain control characters".into());
    }

    let entries = installed()?;
    search_entries(query, &entries)
}

fn search_entries(query: &str, entries: &[Entry]) -> Result<Vec<SearchResult>, String> {
    let needle = query.to_lowercase();
    let mut results: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry.name.to_lowercase().contains(&needle)
                || entry
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.to_lowercase().contains(&needle))
                || entry
                    .path
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&needle)
        })
        .map(|entry| SearchResult {
            name: entry.name.clone(),
            version: String::new(),
            source: PackageSource::ManagedAppImage(entry.path.clone()),
            description: entry.origin.clone().unwrap_or_default(),
        })
        .collect();

    let is_https = query
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
    let is_file =
        needle.ends_with(".appimage") && (query.contains('/') || Path::new(query).is_file());
    if is_https || is_file {
        match import_candidate(query, entries) {
            Ok(Some(candidate)) => results.push(candidate),
            Ok(None) => {}
            Err(error) if results.is_empty() => return Err(error),
            // An invalid or missing import source must not hide installed matches.
            Err(_) => {}
        }
    }
    Ok(results)
}

fn import_candidate(query: &str, entries: &[Entry]) -> Result<Option<SearchResult>, String> {
    let source = Source::parse(query)?;
    if let Source::Local(path) = &source
        && !path.is_file()
    {
        return Err("The AppImage source must be a regular file".into());
    }
    let origin = source.origin()?;
    let already_managed = entries.iter().any(|entry| {
        entry.origin.as_deref() == Some(origin.as_str())
            || entry.path.to_str() == Some(origin.as_str())
    });
    if already_managed {
        return Ok(None);
    }
    Ok(Some(SearchResult {
        name: source.name()?,
        version: String::new(),
        source: PackageSource::AppImageImport(origin.clone()),
        description: origin,
    }))
}

impl Operation {
    pub fn run(&self, mut progress: impl FnMut(&str)) -> Result<String, String> {
        progress("Opening AppImage storage");
        let store = Store::open(true)?.ok_or("AppImage storage is unavailable")?;
        match self {
            Self::Import(input) => {
                let source = Source::parse(input)?;
                let name = source.name()?;
                store.install(&name, source, None, &mut progress)?;
                Ok(format!("Imported {name}"))
            }
            Self::Remove(entries) => store.remove_selected(entries, &mut progress),
            Self::Update(entries) => store.update(entries, &mut progress),
            Self::Replace(entry, input) => {
                progress(&format!("Replacing {}", entry.name));
                store.replace(entry, Source::parse(input)?, &mut progress)?;
                Ok(format!("Replaced {}", entry.name))
            }
        }
    }
}

struct Store {
    root: PathBuf,
    _lock: File,
}

impl Store {
    fn open(write: bool) -> Result<Option<Self>, String> {
        let root = data_path(env::var_os("XDG_DATA_HOME"), env::var_os("HOME"))?;
        Self::at(&root, write)
    }

    fn at(root: &Path, write: bool) -> Result<Option<Self>, String> {
        let parent = root.parent().ok_or("Invalid AppImage storage path")?;
        if write && let Some(data_home) = parent.parent() {
            fs::create_dir_all(data_home)
                .map_err(|error| file_error("create data directory", data_home, error))?;
        }
        if !real_directory(parent, write)? || !real_directory(root, write)? {
            return Ok(None);
        }

        let lock = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(root)
            .map_err(|error| file_error("open AppImage storage", root, error))?;
        if write {
            lock.lock()
        } else {
            lock.lock_shared()
        }
        .map_err(|error| file_error("lock AppImage storage", root, error))?;
        let root = root
            .canonicalize()
            .map_err(|error| file_error("locate AppImage storage", root, error))?;
        let opened = lock
            .metadata()
            .map_err(|error| file_error("inspect AppImage storage", &root, error))?;
        if metadata(&root)?.as_ref().map(identity) != Some(identity(&opened)) {
            return Err("AppImage storage changed while opening it".to_owned());
        }
        Ok(Some(Self { root, _lock: lock }))
    }

    fn installed(&self) -> Result<Vec<Entry>, String> {
        let rows = fs::read_dir(&self.root)
            .map_err(|error| file_error("list AppImage storage", &self.root, error))?;
        let mut entries = Vec::new();
        for row in rows {
            let row =
                row.map_err(|error| file_error("read AppImage storage", &self.root, error))?;
            if let Some(name) = row.file_name().to_str()
                && let Some(entry) = self.entry(name)?
            {
                entries.push(entry);
            }
        }
        entries.sort_unstable_by(|first, second| first.name.cmp(&second.name));
        Ok(entries)
    }

    fn entry(&self, name: &str) -> Result<Option<Entry>, String> {
        if !valid_name(name) {
            return Ok(None);
        }
        let directory = self.root.join(name);
        let Some(directory_metadata) = metadata(&directory)?.filter(Metadata::is_dir) else {
            return Ok(None);
        };
        let manifest_path = directory.join(MANIFEST);
        let Some(manifest_file) = open_managed_file(&manifest_path)? else {
            return Ok(None);
        };
        let mut contents = String::new();
        if manifest_file
            .take(64 * 1024)
            .read_to_string(&mut contents)
            .is_err()
        {
            return Ok(None);
        }
        let Ok(manifest) = toml::from_str::<Manifest>(&contents) else {
            return Ok(None);
        };
        if manifest.name != name || manifest.origin.as_deref().is_some_and(has_control) {
            return Ok(None);
        }
        let path = directory.join(image_filename(name));
        let Some(image_metadata) = metadata(&path)?.filter(single_link_file) else {
            return Ok(None);
        };
        Ok(Some(Entry {
            name: name.to_owned(),
            path,
            origin: manifest.origin,
            directory_id: identity(&directory_metadata),
            image_id: identity(&image_metadata),
        }))
    }

    fn current_directory(&self, entry: &Entry) -> Result<PathBuf, String> {
        if self.entry(&entry.name)?.as_ref() != Some(entry) {
            return Err(format!(
                "{} has changed. Refresh AppImages and try again.",
                entry.name
            ));
        }
        let directory = self.root.join(&entry.name);
        let image = image_filename(&entry.name);
        let files = fs::read_dir(&directory)
            .map_err(|error| file_error("read managed AppImage directory", &directory, error))?;
        for file in files {
            let file = file.map_err(|error| {
                file_error("read managed AppImage directory", &directory, error)
            })?;
            if file.file_name() != MANIFEST && file.file_name() != image.as_str() {
                return Err(format!(
                    "{} contains other files. Move them before changing this AppImage.",
                    directory.display()
                ));
            }
        }
        Ok(directory)
    }

    fn replace(
        &self,
        entry: &Entry,
        source: Source,
        progress: &mut impl FnMut(&str),
    ) -> Result<(), String> {
        self.current_directory(entry)?;
        self.install(&entry.name, source, Some(entry), progress)
    }

    fn remove(&self, entry: &Entry, progress: &mut impl FnMut(&str)) -> Result<(), String> {
        progress(&format!("Removing {}", entry.name));
        let directory = self.current_directory(entry)?;
        let staging = self.staging()?;
        let retired = staging.path().join("removed");
        rename_directory(&directory, &retired, libc::RENAME_NOREPLACE)
            .map_err(|error| file_error("remove AppImage", &directory, error))?;
        delete_retired(staging, &retired, &entry.name)
    }

    fn remove_selected(
        &self,
        entries: &[Entry],
        progress: &mut impl FnMut(&str),
    ) -> Result<String, String> {
        if entries.is_empty() {
            return Err("Select at least one managed AppImage".into());
        }
        let mut failures = Vec::new();
        for entry in entries {
            match self.remove(entry, progress) {
                Ok(()) => progress(&format!("Removed {}", entry.name)),
                Err(error) => {
                    progress(&format!("Could not remove {}", entry.name));
                    failures.push(format!("{}: {error}", entry.name));
                }
            }
        }

        let removed = entries.len() - failures.len();
        let summary = format!("Removed {removed} of {} selected AppImages", entries.len());
        if failures.is_empty() {
            Ok(summary)
        } else {
            Err(format!("{summary}. {}", failures.join(". ")))
        }
    }

    fn update(&self, entries: &[Entry], progress: &mut impl FnMut(&str)) -> Result<String, String> {
        if entries.is_empty() {
            return Err("No AppImages to refresh".to_owned());
        }
        let mut refreshed = 0;
        let mut failures = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            progress(&format!(
                "Refreshing {} ({}/{})",
                entry.name,
                index + 1,
                entries.len()
            ));
            let result = entry
                .origin
                .as_deref()
                .ok_or_else(|| "Set an update source using Replace".to_owned())
                .and_then(Source::parse)
                .and_then(|source| self.replace(entry, source, progress));
            match result {
                Ok(()) => {
                    refreshed += 1;
                    progress(&format!("Refreshed {}", entry.name));
                }
                Err(error) => {
                    progress(&format!("Could not refresh {}", entry.name));
                    failures.push(format!("{}: {error}", entry.name));
                }
            }
        }
        let summary = format!("Refreshed {refreshed} of {} AppImages", entries.len());
        if failures.is_empty() {
            Ok(summary)
        } else {
            Err(format!("{summary}. {}", failures.join(". ")))
        }
    }

    fn install(
        &self,
        name: &str,
        source: Source,
        previous: Option<&Entry>,
        progress: &mut impl FnMut(&str),
    ) -> Result<(), String> {
        let destination = self.root.join(name);
        if previous.is_none() && metadata(&destination)?.is_some() {
            return Err(format!("An AppImage entry named {name} already exists"));
        }
        let staging = self.staging()?;
        let bundle = staging.path().join("bundle");
        DirBuilder::new()
            .mode(0o700)
            .create(&bundle)
            .map_err(|error| file_error("stage AppImage", &bundle, error))?;
        let image_path = bundle.join(image_filename(name));
        let mut image = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&image_path)
            .map_err(|error| file_error("stage AppImage", &image_path, error))?;
        source.transfer(&self.root, &mut image, name, progress)?;
        progress(&format!("Verifying the AppImage header for {name}"));
        validate_header(&image)?;
        progress(&format!("Installing {name}"));
        image
            .set_permissions(fs::Permissions::from_mode(0o700))
            .and_then(|()| image.sync_all())
            .map_err(|error| file_error("save AppImage", &image_path, error))?;

        let manifest = Manifest {
            name: name.to_owned(),
            origin: Some(source.origin()?),
        };
        let contents = toml::to_string(&manifest)
            .map_err(|error| format!("Could not encode AppImage origin ({error})"))?;
        let manifest_path = bundle.join(MANIFEST);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&manifest_path)
            .and_then(|mut file| {
                file.write_all(contents.as_bytes())?;
                file.sync_all()
            })
            .map_err(|error| file_error("save AppImage origin", &manifest_path, error))?;

        // Publish the binary and its source in one rename.
        let flags = if let Some(previous) = previous {
            self.current_directory(previous)?;
            libc::RENAME_EXCHANGE
        } else {
            libc::RENAME_NOREPLACE
        };
        rename_directory(&bundle, &destination, flags)
            .map_err(|error| file_error("publish AppImage", &destination, error))?;
        if previous.is_some() {
            delete_retired(staging, &bundle, name)?;
        }
        Ok(())
    }

    fn staging(&self) -> Result<TempDir, String> {
        tempfile::Builder::new()
            .prefix(".stage-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&self.root)
            .map_err(|error| file_error("stage AppImage", &self.root, error))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    origin: Option<String>,
}

enum Source {
    Local(PathBuf),
    Https(String),
}

impl Source {
    fn parse(input: &str) -> Result<Self, String> {
        if input.is_empty() || has_control(input) || input.len() > 8192 {
            return Err("Enter a local AppImage path or a direct HTTPS URL".to_owned());
        }
        if input
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        {
            return Ok(Self::Https(input.to_owned()));
        }
        if input.contains("://") {
            return Err("AppImage download URLs must use HTTPS".to_owned());
        }
        let path = match input.strip_prefix("~/") {
            Some(relative) => env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|home| home.is_absolute())
                .ok_or("Set HOME to an absolute path to use ~/ in an AppImage path")?
                .join(relative),
            None => PathBuf::from(input),
        };
        let path = path
            .canonicalize()
            .map_err(|error| file_error("find AppImage source", &path, error))?;
        let source = Self::Local(path);
        source.origin()?;
        Ok(source)
    }

    fn origin(&self) -> Result<String, String> {
        match self {
            Self::Local(path) => path
                .to_str()
                .filter(|path| !has_control(path))
                .map(str::to_owned)
                .ok_or_else(|| "The AppImage source path must be printable UTF-8".to_owned()),
            Self::Https(url) => Ok(url.clone()),
        }
    }

    fn name(&self) -> Result<String, String> {
        let filename = match self {
            Self::Local(path) => path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
            Self::Https(url) => url[8..]
                .split(['?', '#'])
                .next()
                .and_then(|location| location.split_once('/'))
                .and_then(|(_, path)| percent_decoded(path.rsplit('/').next()?)),
        }
        .ok_or("The AppImage source must include a file name")?;
        let name = filename
            .len()
            .checked_sub(EXTENSION.len())
            .and_then(|index| filename.get(index..).zip(filename.get(..index)))
            .filter(|(suffix, _)| suffix.eq_ignore_ascii_case(EXTENSION))
            .map_or(filename.as_str(), |(_, stem)| stem);
        if !valid_name(name) {
            return Err("AppImage names must start with a letter or number and use letters, numbers, spaces, dots, hyphens, underscores or plus signs".to_owned());
        }
        Ok(name.to_owned())
    }

    fn transfer(
        &self,
        root: &Path,
        destination: &mut File,
        name: &str,
        progress: &mut impl FnMut(&str),
    ) -> Result<(), String> {
        match self {
            Self::Local(path) => {
                progress(&format!("Copying {name} from a local file"));
                if path.starts_with(root) {
                    return Err("Choose a source outside managed AppImage storage".to_owned());
                }
                let mut source = open_file(path)
                    .map_err(|error| file_error("open AppImage source", path, error))?;
                let metadata = source
                    .metadata()
                    .map_err(|error| file_error("read AppImage source", path, error))?;
                if !metadata.is_file() {
                    return Err("The AppImage source must be a regular file".to_owned());
                }
                validate_header(&source)?;
                let copied = io::copy(
                    &mut Read::by_ref(&mut source).take(metadata.len()),
                    destination,
                )
                .map_err(|error| file_error("copy AppImage source", path, error))?;
                if copied != metadata.len()
                    || source
                        .read(&mut [0])
                        .map_err(|error| file_error("read AppImage source", path, error))?
                        != 0
                {
                    return Err("The AppImage source size changed while copying".to_owned());
                }
                Ok(())
            }
            Self::Https(url) => download(url, destination, name, progress),
        }
    }
}

fn download(
    url: &str,
    destination: &File,
    name: &str,
    progress: &mut impl FnMut(&str),
) -> Result<(), String> {
    progress(&format!("Downloading {name}"));
    let output = destination
        .try_clone()
        .map_err(|error| format!("Could not open staged AppImage download ({error})"))?;
    let mut curl = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--fail",
            "--location",
            "--globoff",
            "--disallow-username-in-url",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            "300",
            "--max-redirs",
            "10",
            "--url",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => {
                "Install curl to download AppImages from HTTPS URLs".to_owned()
            }
            _ => format!("Could not start AppImage download ({error})"),
        })?;
    let mut downloaded = 0;
    let status = loop {
        let status = match curl.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = curl.kill();
                let _ = curl.wait();
                return Err(format!("Could not monitor AppImage download ({error})"));
            }
        };
        if let Ok(metadata) = destination.metadata()
            && metadata.len() > downloaded
        {
            downloaded = metadata.len();
            progress(&format!("Downloading {name} ({downloaded} bytes received)"));
        }
        if let Some(status) = status {
            break status;
        }
        thread::sleep(Duration::from_millis(250));
    };
    if !status.success() {
        // Curl's diagnostics can echo source or redirect URLs containing secrets.
        let detail = match status.code() {
            Some(6) => "Could not resolve the download host",
            Some(7) => "Could not connect to the download host",
            Some(22) => "The server returned an HTTP error",
            Some(28) => "The download timed out",
            Some(47) => "The download followed too many redirects",
            Some(60) => "Could not verify the server certificate",
            _ => return Err(format!("AppImage download failed ({status})")),
        };
        return Err(format!("AppImage download failed ({status}). {detail}"));
    }
    Ok(())
}

fn data_path(data_home: Option<OsString>, home: Option<OsString>) -> Result<PathBuf, String> {
    let parent = data_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        })
        .ok_or("Set XDG_DATA_HOME or HOME to an absolute path to manage AppImages")?;
    Ok(parent.join("archoxide/appimages"))
}

fn real_directory(path: &Path, create: bool) -> Result<bool, String> {
    let existing = metadata(path)?;
    if !create && existing.is_none() {
        return Ok(false);
    }
    if existing.is_none() {
        match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(file_error("create AppImage storage", path, error)),
        }
    }
    if !metadata(path)?.is_some_and(|metadata| metadata.is_dir()) {
        return Err(format!(
            "AppImage storage needs a real directory at {}",
            path.display()
        ));
    }
    Ok(true)
}

fn metadata(path: &Path) -> Result<Option<Metadata>, String> {
    match path.symlink_metadata() {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(file_error("inspect AppImage path", path, error)),
    }
}

fn open_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

fn open_managed_file(path: &Path) -> Result<Option<File>, String> {
    if !metadata(path)?.as_ref().is_some_and(single_link_file) {
        return Ok(None);
    }
    let file = open_file(path).map_err(|error| file_error("open AppImage origin", path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| file_error("inspect AppImage origin", path, error))?;
    Ok(single_link_file(&metadata).then_some(file))
}

fn single_link_file(metadata: &Metadata) -> bool {
    metadata.is_file() && metadata.nlink() == 1
}

fn identity(metadata: &Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

fn image_filename(name: &str) -> String {
    format!("{name}{EXTENSION}")
}

fn delete_retired(staging: TempDir, directory: &Path, name: &str) -> Result<(), String> {
    let staging_path = staging.keep();
    fs::remove_file(directory.join(image_filename(name)))
        .and_then(|()| fs::remove_file(directory.join(MANIFEST)))
        .and_then(|()| fs::remove_dir(directory))
        .and_then(|()| fs::remove_dir(staging_path))
        .map_err(|error| file_error("clean up the previous AppImage", directory, error))
}

fn valid_name(name: &str) -> bool {
    name.len() <= 240
        && name.chars().next().is_some_and(char::is_alphanumeric)
        && name.trim_end() == name
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-' | '+'))
}

fn has_control(value: &str) -> bool {
    value
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
}

fn percent_decoded(value: &str) -> Option<String> {
    let mut bytes = value.bytes();
    let mut decoded = Vec::new();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = char::from(bytes.next()?).to_digit(16)?;
            let low = char::from(bytes.next()?).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
        } else {
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).ok()
}

fn validate_header(mut file: &File) -> Result<(), String> {
    let mut header = [0; 64];
    let invalid = || "The file does not have a valid AppImage ELF header".to_owned();
    let read_error = |error: io::Error| match error.kind() {
        io::ErrorKind::UnexpectedEof => invalid(),
        _ => format!("Could not read AppImage header ({error})"),
    };
    file.rewind()
        .map_err(|error| format!("Could not read AppImage header ({error})"))?;
    file.read_exact(&mut header[..16]).map_err(read_error)?;
    let header_size = match header[4] {
        1 => 52,
        2 => 64,
        _ => return Err(invalid()),
    };
    file.read_exact(&mut header[16..header_size])
        .map_err(read_error)?;
    let read_u16 = match header[5] {
        1 => u16::from_le_bytes,
        2 => u16::from_be_bytes,
        _ => return Err(invalid()),
    };
    let version = if header[5] == 1 {
        [1, 0, 0, 0]
    } else {
        [0, 0, 0, 1]
    };
    if header[..4] != *b"\x7fELF"
        || header[6] != 1
        || header[8..10] != *b"AI"
        || !matches!(header[10], 1 | 2)
        || !matches!(read_u16([header[16], header[17]]), 2 | 3)
        || read_u16([header[18], header[19]]) == 0
        || header[20..24] != version
        || read_u16([header[header_size - 12], header[header_size - 11]]) != header_size as u16
        || file
            .metadata()
            .map_err(|error| format!("Could not inspect AppImage ({error})"))?
            .len()
            <= header_size as u64
    {
        return Err(invalid());
    }
    file.rewind()
        .map_err(|error| format!("Could not seek in AppImage ({error})"))
}

fn rename_directory(source: &Path, destination: &Path, flags: libc::c_uint) -> io::Result<()> {
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // Both paths and their containing storage remain alive for the syscall.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            flags,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn file_error(action: &str, path: &Path, error: io::Error) -> String {
    format!("Could not {action} at {} ({error})", path.display())
}

#[cfg(test)]
#[path = "../tests/unit/appimage.rs"]
mod tests;
