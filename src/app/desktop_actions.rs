use std::time::{Duration, Instant};

use super::{App, Field, KeyCode, KeyEvent, KeyEventKind, ReadState, Request, View, plain_key};
use crate::{desktop_apps::Entry, source_views::matches_filter};

const LEGACY_ENTER_QUIET_PERIOD: Duration = Duration::from_millis(800);

#[derive(Debug, Default)]
pub struct DesktopApps {
    entries: Vec<Entry>,
    pub(super) filter: String,
    pub state: ReadState,
    pub(super) revision: u64,
    pub(super) launching: bool,
    key_event_reporting: bool,
    last_legacy_enter: Option<Instant>,
}

impl DesktopApps {
    pub(super) fn accept_key(&mut self, key: KeyEvent, now: Instant) -> bool {
        if self.key_event_reporting || key.kind != KeyEventKind::Press {
            return true;
        }
        if key.code != KeyCode::Enter {
            self.last_legacy_enter = None;
            return true;
        }
        // Older terminals send identical bytes for a press and its autorepeats.
        // Restart the quiet period even for a suppressed Enter.
        !plain_key(key)
            || self
                .last_legacy_enter
                .replace(now)
                .is_none_or(|last| now.duration_since(last) >= LEGACY_ENTER_QUIET_PERIOD)
    }
}

impl App {
    pub fn enable_key_event_reporting(&mut self) {
        self.desktop_apps.key_event_reporting = true;
    }

    pub fn desktop_app_rows(&self) -> impl Iterator<Item = &Entry> + '_ {
        let filter = self.source_filter(Field::DesktopAppFilter, &self.desktop_apps.filter);
        self.desktop_apps
            .entries
            .iter()
            .filter(move |entry| matches_filter(&filter, &[&entry.name, &entry.id, &entry.comment]))
    }

    pub fn selected_desktop_app(&self) -> Option<&Entry> {
        self.desktop_app_rows()
            .nth(self.selections[View::Apps.index()].selected()?)
    }

    pub(super) fn refresh_desktop_apps(&mut self) {
        self.desktop_apps.state = ReadState::Loading;
        self.desktop_apps.revision = self.desktop_apps.revision.wrapping_add(1);
        self.reads
            .submit(Request::DesktopApps(self.desktop_apps.revision));
    }

    pub(super) fn load_desktop_apps(&mut self, result: Result<Vec<Entry>, String>) {
        match result {
            Ok(entries) => {
                let selected = self.selected_desktop_app().map(|entry| entry.id.clone());
                self.desktop_apps.entries = entries;
                self.desktop_apps.state = ReadState::Ready;
                let row =
                    selected.and_then(|id| self.desktop_app_rows().position(|app| app.id == id));
                self.selections[View::Apps.index()].select(row);
                self.clamp_selection(View::Apps);
            }
            Err(error) => self.desktop_apps.state = ReadState::Failed(error),
        }
    }

    pub(super) fn launch_selected_desktop_app(&mut self) {
        if self.desktop_apps.launching || self.desktop_apps.state != ReadState::Ready {
            return;
        }
        if let Some(entry) = self.selected_desktop_app().cloned() {
            self.notice = Some(format!("Opening {}…", entry.name));
            self.desktop_apps.launching = true;
            self.reads.submit(Request::LaunchDesktopApp(entry));
        }
    }
}
