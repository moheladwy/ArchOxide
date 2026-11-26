use std::{collections::HashSet, hash::Hash, path::PathBuf};

use crate::{app::ReadState, appimage, flatpak};

pub type FlatpakKey = (flatpak::Installation, String);

#[derive(Debug)]
pub struct Catalog<T, K> {
    pub entries: Vec<T>,
    pub filter: String,
    pub marks: HashSet<K>,
    pub state: ReadState,
    pub revision: u64,
}

impl<T, K> Default for Catalog<T, K> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            filter: String::new(),
            marks: HashSet::new(),
            state: ReadState::Idle,
            revision: 0,
        }
    }
}

impl<T, K: Eq + Hash> Catalog<T, K> {
    pub fn invalidate(&mut self) {
        self.state = ReadState::Idle;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn begin_read(&mut self) -> u64 {
        self.state = ReadState::Loading;
        self.revision = self.revision.wrapping_add(1);
        self.revision
    }

    pub fn apply(&mut self, result: Result<Vec<T>, String>, key: impl Fn(&T) -> K) {
        match result {
            Ok(entries) => {
                self.marks
                    .retain(|marked| entries.iter().any(|entry| key(entry) == *marked));
                self.entries = entries;
                self.state = ReadState::Ready;
            }
            Err(error) => self.state = ReadState::Failed(error),
        }
    }

    pub fn toggle(&mut self, key: K) {
        if !self.marks.remove(&key) {
            self.marks.insert(key);
        }
    }

    pub fn selected<'a>(&'a self, highlighted: Option<&'a T>, key: impl Fn(&T) -> K) -> Vec<&'a T> {
        if self.marks.is_empty() {
            highlighted.into_iter().collect()
        } else {
            self.entries
                .iter()
                .filter(|entry| self.marks.contains(&key(entry)))
                .collect()
        }
    }
}

#[derive(Debug, Default)]
pub struct Sources {
    pub flatpaks: Catalog<flatpak::InstalledApp, FlatpakKey>,
    pub flatpak_query: Option<String>,
    pub flatpak_results: Vec<flatpak::SearchApp>,
    pub flatpak_search: ReadState,
    pub flatpak_search_id: u64,
    pub images: Catalog<appimage::Entry, PathBuf>,
}

pub fn flatpak_key(app: &flatpak::InstalledApp) -> FlatpakKey {
    (app.installation.clone(), app.reference.clone())
}

pub fn matches_filter(query: &str, values: &[&str]) -> bool {
    query.is_empty()
        || values
            .iter()
            .any(|value| value.to_lowercase().contains(query))
}
