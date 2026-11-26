use super::{Action, App, Draft, Field, Pane, ReadState, Request, Response, Transaction, View};
use crate::{
    appimage, flatpak,
    source_views::{flatpak_key, matches_filter},
};

impl App {
    pub fn flatpak_rows(&self) -> impl Iterator<Item = &flatpak::InstalledApp> + '_ {
        let filter = self.source_filter(Field::FlatpakFilter, &self.sources.flatpaks.filter);
        self.sources.flatpaks.entries.iter().filter(move |app| {
            matches_filter(&filter, &[&app.name, &app.id, &app.version, &app.remote])
        })
    }

    pub fn image_rows(&self) -> impl Iterator<Item = &appimage::Entry> + '_ {
        let filter = self.source_filter(Field::ImageFilter, &self.sources.images.filter);
        self.sources.images.entries.iter().filter(move |entry| {
            matches_filter(
                &filter,
                &[&entry.name, entry.path.to_str().unwrap_or_default()],
            )
        })
    }

    pub(super) fn source_filter(&self, field: Field, committed: &str) -> String {
        self.draft
            .as_ref()
            .filter(|draft| draft.field == field)
            .map_or(committed, |draft| draft.text.as_str())
            .to_lowercase()
    }

    pub fn selected_flatpak(&self) -> Option<&flatpak::InstalledApp> {
        self.flatpak_rows()
            .nth(self.selections[View::Flatpak.index()].selected()?)
    }

    pub fn selected_flatpak_result(&self) -> Option<&flatpak::SearchApp> {
        self.sources
            .flatpak_results
            .get(self.selections[View::Flatpak.index()].selected()?)
    }

    pub fn selected_image(&self) -> Option<&appimage::Entry> {
        self.image_rows()
            .nth(self.selections[View::AppImages.index()].selected()?)
    }

    pub(super) fn open_source(&mut self, view: View) {
        match view {
            View::Flatpak if self.sources.flatpaks.state == ReadState::Idle => {
                self.refresh_flatpak_inventory()
            }
            View::AppImages if self.sources.images.state == ReadState::Idle => {
                self.refresh_image_inventory()
            }
            _ => {}
        }
    }

    pub(super) fn refresh_flatpak_inventory(&mut self) {
        let revision = self.sources.flatpaks.begin_read();
        self.reads.submit(Request::FlatpakInventory(revision));
    }

    pub(super) fn refresh_image_inventory(&mut self) {
        let revision = self.sources.images.begin_read();
        self.reads.submit(Request::ImageInventory(revision));
    }

    pub(super) fn leave_flatpak_search(&mut self) {
        self.sources.flatpak_query = None;
        self.sources.flatpak_results.clear();
        self.sources.flatpak_search = ReadState::Idle;
        self.sources.flatpak_search_id = self.sources.flatpak_search_id.wrapping_add(1);
        self.reads.clear_flatpak_search();
        self.selections[View::Flatpak.index()].select(None);
        self.clamp_selection(View::Flatpak);
    }

    pub(super) fn search_flatpaks(&mut self, query: String) {
        let query = query.trim();
        if query.is_empty() {
            self.leave_flatpak_search();
            return;
        }
        self.sources.flatpak_query = Some(query.to_owned());
        self.sources.flatpak_results.clear();
        self.sources.flatpak_search = ReadState::Loading;
        self.sources.flatpak_search_id = self.sources.flatpak_search_id.wrapping_add(1);
        self.selections[View::Flatpak.index()].select(None);
        self.reads.submit(Request::FlatpakSearch(
            self.sources.flatpak_search_id,
            query.to_owned(),
        ));
    }

    pub(super) fn apply_source_action(&mut self, action: Action) -> bool {
        match self.view {
            View::Flatpak => self.flatpak_action(action),
            View::AppImages => self.image_action(action),
            _ => false,
        }
    }

    fn flatpak_action(&mut self, action: Action) -> bool {
        let browsing = self.sources.flatpak_query.is_some();
        match action {
            Action::SearchFlatpak => {
                self.draft = Some(Draft {
                    field: Field::FlatpakSearch,
                    text: self.sources.flatpak_query.clone().unwrap_or_default(),
                });
                self.focus = Pane::List;
            }
            Action::RefreshInventory => {
                if let Some(query) = self.sources.flatpak_query.clone() {
                    self.search_flatpaks(query);
                }
                self.refresh_flatpak_inventory();
            }
            Action::Install if browsing => {
                self.confirmation = self
                    .selected_flatpak_result()
                    .cloned()
                    .map(|app| Transaction::Flatpak(flatpak::Operation::Install(app)));
            }
            Action::Upgrade if !matches!(self.sources.flatpaks.state, ReadState::Failed(_)) => {
                self.confirmation = Some(Transaction::Flatpak(flatpak::Operation::UpdateAll));
            }
            Action::ToggleMark if !browsing => {
                if let Some(key) = self.selected_flatpak().map(flatpak_key) {
                    self.sources.flatpaks.toggle(key);
                }
            }
            Action::ClearMarks if !browsing => self.sources.flatpaks.marks.clear(),
            Action::Remove | Action::UpgradeSelected | Action::UpgradeExcept
                if !browsing && self.sources.flatpaks.state == ReadState::Ready =>
            {
                let selected = self
                    .sources
                    .flatpaks
                    .selected(self.selected_flatpak(), flatpak_key)
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    self.notice = Some("Select at least one installed Flatpak.".into());
                } else {
                    let operation = match action {
                        Action::Remove => flatpak::Operation::Remove(selected),
                        Action::UpgradeExcept => flatpak::Operation::UpdateExcept(selected),
                        _ => flatpak::Operation::Update(selected),
                    };
                    self.confirmation = Some(Transaction::Flatpak(operation));
                }
            }
            _ => return false,
        }
        true
    }

    fn image_action(&mut self, action: Action) -> bool {
        match action {
            Action::RefreshInventory => self.refresh_image_inventory(),
            Action::ToggleMark => {
                if let Some(path) = self.selected_image().map(|entry| entry.path.clone()) {
                    self.sources.images.toggle(path);
                }
            }
            Action::ClearMarks => self.sources.images.marks.clear(),
            Action::Install => {
                self.draft = Some(Draft {
                    field: Field::ImageImport,
                    text: String::new(),
                });
                self.focus = Pane::List;
            }
            Action::ReplaceAppImage
                if self.sources.images.state == ReadState::Ready
                    && self.selected_image().is_some() =>
            {
                self.draft = Some(Draft {
                    field: Field::ImageReplace,
                    text: String::new(),
                });
                self.focus = Pane::List;
            }
            Action::Remove if self.sources.images.state == ReadState::Ready => {
                let entries: Vec<_> = self
                    .sources
                    .images
                    .selected(self.selected_image(), |entry| entry.path.clone())
                    .into_iter()
                    .cloned()
                    .collect();
                if entries.is_empty() {
                    self.notice = Some("Select at least one managed AppImage.".into());
                } else {
                    self.confirmation =
                        Some(Transaction::AppImage(appimage::Operation::Remove(entries)));
                }
            }
            Action::Upgrade | Action::UpgradeSelected | Action::UpgradeExcept
                if self.sources.images.state == ReadState::Ready =>
            {
                self.confirm_image_update(action);
            }
            _ => return false,
        }
        true
    }

    fn confirm_image_update(&mut self, action: Action) {
        let catalog = &self.sources.images;
        let targets = if action == Action::Upgrade {
            catalog.entries.iter().collect::<Vec<_>>()
        } else {
            let selected = catalog.selected(self.selected_image(), |entry| entry.path.clone());
            if selected.is_empty() {
                self.notice = Some("Select at least one managed AppImage.".into());
                return;
            }
            if action == Action::UpgradeExcept {
                catalog
                    .entries
                    .iter()
                    .filter(|entry| !selected.iter().any(|target| target.path == entry.path))
                    .collect()
            } else {
                selected
            }
        };
        if targets.is_empty() {
            self.notice = Some("No managed AppImages to update.".into());
        } else {
            let targets = targets.into_iter().cloned().collect();
            self.confirmation = Some(Transaction::AppImage(appimage::Operation::Update(targets)));
        }
    }

    pub(super) fn confirm_image_source(&mut self, field: Field, text: String) {
        let source = text.trim().to_owned();
        if source.is_empty() {
            self.notice = Some("Enter a local AppImage path or direct HTTPS URL.".into());
            return;
        }
        let operation = if field == Field::ImageReplace {
            let Some(entry) = self.selected_image().cloned() else {
                return;
            };
            appimage::Operation::Replace(entry, source)
        } else {
            appimage::Operation::Import(source)
        };
        self.confirmation = Some(Transaction::AppImage(operation));
    }

    pub(super) fn apply_source_read(&mut self, response: Response) -> bool {
        match response {
            Response::FlatpakInventory(id, result) if id == self.sources.flatpaks.revision => {
                let selected = self.selected_flatpak().map(flatpak_key);
                self.sources.flatpaks.apply(result, flatpak_key);
                if self.sources.flatpak_query.is_none() {
                    let row = selected.and_then(|key| {
                        self.flatpak_rows()
                            .position(|entry| flatpak_key(entry) == key)
                    });
                    self.selections[View::Flatpak.index()].select(row);
                    self.clamp_selection(View::Flatpak);
                }
            }
            Response::FlatpakSearch(id, result)
                if id == self.sources.flatpak_search_id && self.sources.flatpak_query.is_some() =>
            {
                match result {
                    Ok(apps) => {
                        self.sources.flatpak_results = apps;
                        self.sources.flatpak_search = ReadState::Ready;
                    }
                    Err(error) => self.sources.flatpak_search = ReadState::Failed(error),
                }
                self.clamp_selection(View::Flatpak);
            }
            Response::ImageInventory(id, result) if id == self.sources.images.revision => {
                let selected = self.selected_image().map(|entry| entry.path.clone());
                self.sources
                    .images
                    .apply(result, |entry| entry.path.clone());
                let row = selected
                    .and_then(|path| self.image_rows().position(|entry| entry.path == path));
                self.selections[View::AppImages.index()].select(row);
                self.clamp_selection(View::AppImages);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app/source_actions.rs"]
mod tests;
