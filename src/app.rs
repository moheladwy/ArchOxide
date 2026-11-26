use std::{collections::HashSet, path::PathBuf, time::Instant};

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::ListState,
};

use crate::{
    config::{Config, Density, Theme},
    input::{Action, Bindings, plain_key},
    packages::{
        self, Availability, InstalledPackage, Inventory, SearchResult, SearchSource, Source,
        Transaction,
    },
    picker::{Choice, Kind as PickerKind, Outcome as PickerOutcome, Picker},
    reads::{Reads, Request, Response, SEARCH_SOURCES, search_index},
    source_views::Sources,
};

mod source_actions;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Installed,
    Search,
    Flatpak,
    AppImages,
    Settings,
}

impl View {
    pub const ALL: [Self; 5] = [
        Self::Installed,
        Self::Search,
        Self::Flatpak,
        Self::AppImages,
        Self::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::Search => "Search",
            Self::Flatpak => "Flatpak",
            Self::AppImages => "AppImages",
            Self::Settings => "Settings",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Theme,
    Density,
    DetailsWidth,
    FontFamily,
    FontSize,
    Aur,
    Flatpak,
    AppImages,
    Save,
    Reset,
}

impl Setting {
    pub const ALL: [Self; 10] = [
        Self::Theme,
        Self::Density,
        Self::DetailsWidth,
        Self::FontFamily,
        Self::FontSize,
        Self::Aur,
        Self::Flatpak,
        Self::AppImages,
        Self::Save,
        Self::Reset,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Density => "UI density",
            Self::DetailsWidth => "Details width",
            Self::FontFamily => "Font family",
            Self::FontSize => "Font size",
            Self::Aur => "AUR support",
            Self::Flatpak => "Flatpak support",
            Self::AppImages => "AppImages support",
            Self::Save => "Save settings",
            Self::Reset => "Reset to defaults",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Theme => {
                "Enter opens the theme picker. Highlight a theme to preview it, then Save to apply it."
            }
            Self::Density => "Enter changes how much space each row uses.",
            Self::DetailsWidth => "Enter edits the details pane width (25–60 percent).",
            Self::FontFamily => {
                "Enter opens the installed font picker. Saved fonts apply on the next --kitty launch."
            }
            Self::FontSize => {
                "Enter edits the font size in points for the next --kitty launch (6–48). Blank inherits Kitty's setting."
            }
            Self::Aur => {
                "Enter toggles AUR search, installs, and upgrades. AUR support requires yay."
            }
            Self::Flatpak => {
                "Enter toggles the Flatpak view and operations. Save to apply the change."
            }
            Self::AppImages => {
                "Enter toggles the AppImages view and operations. Save to apply the change."
            }
            Self::Save => "Enter saves your settings and applies changes to the TUI.",
            Self::Reset => "Enter loads the defaults for review. Save to apply them.",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    List,
    Details,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum ReadState {
    #[default]
    Idle,
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Filter,
    Search,
    FlatpakFilter,
    FlatpakSearch,
    ImageFilter,
    ImageImport,
    ImageReplace,
    Setting(Setting),
}

#[derive(Debug)]
struct Draft {
    field: Field,
    text: String,
}

#[derive(Debug, Default)]
pub struct App {
    pub view: View,
    selections: [ListState; View::ALL.len()],
    pub focus: Pane,
    pub bindings: Bindings,
    pub show_help: bool,
    pub help_scroll: u16,
    pub should_quit: bool,
    pub notice: Option<String>,
    pub confirmation: Option<Transaction>,
    pub confirmation_scroll: u16,
    transaction: Option<Transaction>,
    pub config: Config,
    pub picker: Option<Picker>,
    pub sources: Sources,
    config_path: Option<PathBuf>,
    settings: Config,
    pub installed_state: ReadState,
    pub installed_warning: Option<String>,
    installed: Vec<InstalledPackage>,
    marked: HashSet<String>,
    visible_installed: Vec<usize>,
    availability: Option<Availability>,
    filter: String,
    pub query: String,
    pub yay_available: bool,
    search_results: [Vec<SearchResult>; 2],
    pub search_states: [ReadState; 2],
    search_selection: Option<(Source, String)>,
    draft: Option<Draft>,
    details_visible: bool,
    visible_rows: usize,
    reads: Reads,
    inventory_id: u64,
    search_id: u64,
    font_id: u64,
}

impl App {
    pub fn new(config: Config, config_path: Option<PathBuf>, warning: Option<String>) -> Self {
        Self {
            settings: config.clone(),
            config,
            config_path,
            notice: warning,
            ..Self::default()
        }
    }

    pub fn start_background_reads(&mut self) {
        self.yay_available = packages::yay_available();
        self.refresh_inventory();
    }

    pub fn has_pending_reads(&self) -> bool {
        self.reads.pending()
    }

    pub fn poll_reads(&mut self) -> bool {
        let mut changed = false;
        while let Some(response) = self.reads.receive() {
            changed |= self.apply_read(response);
        }
        changed
    }

    pub fn take_transaction(&mut self) -> Option<Transaction> {
        self.transaction.take()
    }

    pub fn transaction_finished(&mut self, message: String, transaction: &Transaction) {
        self.notice = Some(message);
        if matches!(transaction, Transaction::Flatpak(_)) {
            self.refresh_flatpak_inventory();
            return;
        }
        self.refresh_inventory();
        if self.view == View::Search {
            self.refresh_search();
        } else if !self.query.is_empty() {
            // Refresh when Search is opened again so stale rows cannot be acted on.
            self.invalidate_search();
        }
    }

    pub fn installed_rows(&self) -> impl Iterator<Item = &InstalledPackage> + '_ {
        self.visible_installed
            .iter()
            .map(|&index| &self.installed[index])
    }

    pub fn is_marked(&self, name: &str) -> bool {
        self.marked.contains(name)
    }

    pub fn marked_count(&self) -> usize {
        self.marked.len()
    }

    pub fn search_rows(&self) -> impl Iterator<Item = &SearchResult> + '_ {
        self.search_results.iter().flatten()
    }

    pub fn selected_installed(&self) -> Option<&InstalledPackage> {
        let row = self.selections[View::Installed.index()].selected()?;
        self.visible_installed
            .get(row)
            .map(|&index| &self.installed[index])
    }

    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.search_rows()
            .nth(self.selections[View::Search.index()].selected()?)
    }

    pub fn installed_version(&self, name: &str) -> Option<&str> {
        self.installed
            .binary_search_by(|entry| entry.name.as_str().cmp(name))
            .ok()
            .map(|index| self.installed[index].version.as_str())
    }

    pub fn selected_setting(&self) -> Option<Setting> {
        let row = self.selections[View::Settings.index()].selected()?;
        Setting::ALL.get(row).copied()
    }

    pub fn selection_mut(&mut self) -> &mut ListState {
        &mut self.selections[self.view.index()]
    }

    pub fn availability_label(&self) -> &'static str {
        self.availability.map_or("All", Availability::label)
    }

    pub fn aur_enabled(&self) -> bool {
        self.config.packages.aur_enabled && self.yay_available
    }

    pub fn views(&self) -> impl Iterator<Item = View> + '_ {
        View::ALL
            .into_iter()
            .filter(|&view| self.view_enabled(view))
    }

    fn view_enabled(&self, view: View) -> bool {
        match view {
            View::Flatpak => self.config.packages.flatpak_enabled,
            View::AppImages => self.config.packages.appimage_enabled,
            _ => true,
        }
    }

    pub fn settings_dirty(&self) -> bool {
        self.config != self.settings
    }

    pub fn rendered_theme(&self) -> Theme {
        if let Some(Choice::Theme(theme)) = self.picker.as_ref().and_then(Picker::selected) {
            *theme
        } else {
            self.config.appearance.theme
        }
    }

    pub fn setting_value(&self, setting: Setting) -> String {
        let appearance = &self.settings.appearance;
        match setting {
            Setting::Theme => appearance.theme.label().into(),
            Setting::Density => appearance.density.label().into(),
            Setting::DetailsWidth => format!("{}%", appearance.details_width_percent),
            Setting::FontFamily => appearance
                .font_family
                .clone()
                .unwrap_or_else(|| "Inherit".into()),
            Setting::FontSize => appearance
                .font_size_pt
                .map_or_else(|| "Inherit".into(), |size| format!("{size} pt")),
            Setting::Aur => switch_label(self.settings.packages.aur_enabled),
            Setting::Flatpak => switch_label(self.settings.packages.flatpak_enabled),
            Setting::AppImages => switch_label(self.settings.packages.appimage_enabled),
            Setting::Save | Setting::Reset => String::new(),
        }
    }

    pub fn prompt(&self) -> Option<(&'static str, &str)> {
        if let Some(draft) = &self.draft {
            let title = match draft.field {
                Field::Filter => "Filter installed",
                Field::Search => "Search packages",
                Field::FlatpakFilter => "Filter installed Flatpaks",
                Field::FlatpakSearch => "Search Flatpak remotes",
                Field::ImageFilter => "Filter managed AppImages",
                Field::ImageImport => "Import AppImage from a path or HTTPS URL",
                Field::ImageReplace => "Replace AppImage from a path or HTTPS URL",
                Field::Setting(Setting::DetailsWidth) => "Details width (%)",
                Field::Setting(Setting::FontSize) => "Font size (pt)",
                Field::Setting(setting) => setting.label(),
            };
            return Some((title, &draft.text));
        }
        match self.view {
            View::Installed => Some(("Filter installed", &self.filter)),
            View::Search => Some(("Search packages", &self.query)),
            View::Flatpak => match &self.sources.flatpak_query {
                Some(query) => Some(("Search Flatpak remotes", query)),
                None => Some(("Filter installed Flatpaks", &self.sources.flatpaks.filter)),
            },
            View::AppImages => Some(("Filter managed AppImages", &self.sources.images.filter)),
            View::Settings => None,
        }
    }

    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }

    pub fn update_layout(&mut self, visible_rows: usize, details_visible: bool) {
        self.visible_rows = visible_rows;
        self.details_visible = details_visible;
        if !details_visible {
            self.focus = Pane::List;
        }
        self.clamp_selection(self.view);
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            self.request_quit();
            return;
        }
        if let Some(picker) = &mut self.picker {
            match picker.handle_key(key) {
                Some(PickerOutcome::Choose(choice)) => {
                    match choice {
                        Choice::Theme(theme) => self.settings.appearance.theme = theme,
                        Choice::Font(family) => self.settings.appearance.font_family = family,
                    }
                    self.picker = None;
                }
                Some(PickerOutcome::Close) => self.picker = None,
                None => {}
            }
            return;
        }
        if self.show_help {
            if plain_key(key) {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('?') => self.show_help = false,
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.help_scroll = self.help_scroll.saturating_add(1)
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.help_scroll = self.help_scroll.saturating_sub(1)
                    }
                    _ => {}
                }
            }
            return;
        }
        if self.confirmation.is_some() {
            if plain_key(key) {
                match key.code {
                    KeyCode::Esc if key.kind == KeyEventKind::Press => {
                        self.confirmation = None;
                        self.confirmation_scroll = 0;
                    }
                    KeyCode::Enter if key.kind == KeyEventKind::Press => {
                        match self.confirmation.take() {
                            Some(Transaction::AppImage(operation)) => {
                                self.start_image_operation(operation)
                            }
                            transaction => self.transaction = transaction,
                        }
                        self.confirmation_scroll = 0;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.confirmation_scroll = self.confirmation_scroll.saturating_add(1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.confirmation_scroll = self.confirmation_scroll.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            return;
        }
        if self.draft.is_some() {
            self.edit_text(key);
            return;
        }
        if key.code == KeyCode::Esc && self.bindings.pending_label().is_none() {
            self.notice = None;
            if self.view == View::Flatpak && self.sources.flatpak_query.is_some() {
                self.leave_flatpak_search();
                return;
            }
        }
        if let Some(action) = self.bindings.resolve(key, now)
            && (action != Action::ToggleMark || key.kind == KeyEventKind::Press)
        {
            self.apply_action(action);
        }
    }

    fn edit_text(&mut self, key: KeyEvent) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        if key.code == KeyCode::Char('u') && key.modifiers == KeyModifiers::CONTROL {
            draft.text.clear();
            if draft.field == Field::Filter {
                self.refilter();
            }
            return;
        }
        if !plain_key(key) {
            return;
        }

        match key.code {
            KeyCode::Esc => {
                let filtering = draft.field == Field::Filter;
                self.draft = None;
                if filtering {
                    self.refilter();
                }
                return;
            }
            KeyCode::Enter if key.kind == KeyEventKind::Press => {
                let draft = self.draft.take().expect("an active text prompt");
                match draft.field {
                    Field::Filter => {
                        self.filter = draft.text;
                        self.refilter();
                    }
                    Field::Search => {
                        self.query = draft.text.trim().to_owned();
                        self.refresh_search();
                    }
                    Field::FlatpakFilter => self.sources.flatpaks.filter = draft.text,
                    Field::FlatpakSearch => self.search_flatpaks(draft.text),
                    Field::ImageFilter => self.sources.images.filter = draft.text,
                    Field::ImageImport | Field::ImageReplace => {
                        self.confirm_image_source(draft.field, draft.text)
                    }
                    Field::Setting(setting) => self.set_from_text(setting, draft.text),
                }
                return;
            }
            KeyCode::Backspace => {
                draft.text.pop();
            }
            KeyCode::Char(character) if !character.is_control() => draft.text.push(character),
            _ => return,
        }

        if draft.field == Field::Filter {
            self.refilter();
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.request_quit(),
            Action::Help => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            Action::NextView => self.cycle_view(1),
            Action::PreviousView => self.cycle_view(View::ALL.len() - 1),
            Action::Installed => self.open(View::Installed),
            Action::Flatpak => self.open(View::Flatpak),
            Action::AppImages => self.open(View::AppImages),
            Action::Search => {
                self.open(View::Search);
                self.open_prompt();
            }
            Action::Theme => {
                self.open(View::Settings);
                self.selection_mut().select_first();
            }
            Action::Prompt => self.open_prompt(),
            Action::FocusList => self.focus = Pane::List,
            Action::FocusDetails if self.details_visible => self.focus = Pane::Details,
            Action::CycleAvailability if self.view == View::Installed => self.cycle_availability(),
            Action::RefreshInventory if self.view == View::Installed => self.refresh_inventory(),
            Action::Install if self.view == View::Search => {
                self.confirmation = self.selected_result().cloned().map(Transaction::Install);
            }
            Action::Remove
                if self.view == View::Installed && self.installed_state == ReadState::Ready =>
            {
                self.confirmation = self.selected_installed().cloned().map(Transaction::Remove);
            }
            Action::Upgrade if !matches!(self.view, View::Flatpak | View::AppImages) => {
                self.confirmation = Some(Transaction::Upgrade {
                    aur: self.aur_enabled(),
                })
            }
            Action::ToggleMark if self.view == View::Installed => {
                if let Some(package) = self.selected_installed() {
                    let name = package.name.clone();
                    if !self.marked.remove(&name) {
                        self.marked.insert(name);
                    }
                }
            }
            Action::ClearMarks if self.view == View::Installed => self.marked.clear(),
            Action::UpgradeSelected | Action::UpgradeExcept if self.view == View::Installed => {
                self.confirm_scoped_upgrade(action == Action::UpgradeExcept);
            }
            Action::EditSetting if self.view == View::Settings => {
                if let Some(setting) = self.selected_setting() {
                    self.edit_setting(setting);
                }
            }
            Action::SaveSettings if self.view == View::Settings => self.save_settings(),
            Action::ResetSettings if self.view == View::Settings => self.reset_settings(),
            _ => {
                if !self.apply_source_action(action) && self.focus == Pane::List {
                    self.navigate(action, self.row_count(self.view));
                }
            }
        }
    }

    fn open(&mut self, view: View) {
        if !self.view_enabled(view) {
            self.notice = Some(format!(
                "{} support is off. Enable it in Settings.",
                view.title()
            ));
            return;
        }
        self.view = view;
        self.focus = Pane::List;
        self.clamp_selection(view);
        self.open_source(view);
        if view == View::Search
            && !self.query.is_empty()
            && self.search_states[0] == ReadState::Idle
        {
            self.refresh_search();
        }
    }

    fn confirm_scoped_upgrade(&mut self, exclude: bool) {
        if self.installed_state != ReadState::Ready {
            self.notice = Some("Refresh Installed before choosing packages to update.".into());
            return;
        }
        let selected: Vec<_> = if self.marked.is_empty() {
            self.selected_installed().into_iter().collect()
        } else {
            self.installed
                .iter()
                .filter(|package| self.is_marked(&package.name))
                .collect()
        };
        if selected.is_empty() {
            self.notice = Some("Select at least one installed package.".into());
            return;
        }

        let names = selected
            .iter()
            .map(|package| package.name.clone())
            .collect();
        if exclude {
            self.confirmation = Some(Transaction::UpgradeExcept {
                aur: self.aur_enabled(),
                names,
            });
            return;
        }

        if selected
            .iter()
            .any(|package| package.availability == Availability::Unknown)
        {
            self.notice =
                Some("Refresh Installed to identify the selected package sources.".into());
            return;
        }
        let aur = selected
            .iter()
            .any(|package| package.availability == Availability::Foreign);
        if aur && !self.aur_enabled() {
            self.notice = Some(
                "Updating selected foreign packages requires yay and AUR support in Settings."
                    .into(),
            );
            return;
        }
        self.confirmation = Some(Transaction::UpgradeSelected { aur, names });
    }

    fn cycle_view(&mut self, step: usize) {
        let mut index = self.view.index();
        loop {
            index = (index + step) % View::ALL.len();
            let view = View::ALL[index];
            if self.view_enabled(view) {
                self.open(view);
                break;
            }
        }
    }

    fn open_prompt(&mut self) {
        let (field, text) = match self.view {
            View::Installed => (Field::Filter, self.filter.clone()),
            View::Search => (Field::Search, self.query.clone()),
            View::Flatpak => match &self.sources.flatpak_query {
                Some(query) => (Field::FlatpakSearch, query.clone()),
                None => (Field::FlatpakFilter, self.sources.flatpaks.filter.clone()),
            },
            View::AppImages => (Field::ImageFilter, self.sources.images.filter.clone()),
            View::Settings => return,
        };
        self.draft = Some(Draft { field, text });
        self.focus = Pane::List;
    }

    fn row_count(&self, view: View) -> usize {
        match view {
            View::Installed => self.visible_installed.len(),
            View::Search => self.search_results.iter().map(Vec::len).sum(),
            View::Flatpak if self.sources.flatpak_query.is_some() => {
                self.sources.flatpak_results.len()
            }
            View::Flatpak => self.flatpak_rows().count(),
            View::AppImages => self.image_rows().count(),
            View::Settings => Setting::ALL.len(),
        }
    }

    fn clamp_selection(&mut self, view: View) {
        let last = self.row_count(view).checked_sub(1);
        let selection = &mut self.selections[view.index()];
        selection.select(last.map(|last| selection.selected().unwrap_or(0).min(last)));
    }

    fn navigate(&mut self, action: Action, row_count: usize) {
        let half_page = (self.visible_rows / 2).max(1);
        let selection = &mut self.selections[self.view.index()];
        let Some(last) = row_count.checked_sub(1) else {
            selection.select(None);
            return;
        };
        let row = selection.selected().unwrap_or(0).min(last);
        let next = match action {
            Action::Up => row.saturating_sub(1),
            Action::Down => row + 1,
            Action::First => 0,
            Action::Last => last,
            Action::HalfPageUp => row.saturating_sub(half_page),
            Action::HalfPageDown => row + half_page,
            _ => return,
        };
        selection.select(Some(next.min(last)));
        if self.view == View::Search {
            self.search_selection = None;
        }
    }

    fn cycle_availability(&mut self) {
        self.availability = match self.availability {
            None => Some(Availability::Repository),
            Some(Availability::Repository) => Some(Availability::Foreign),
            Some(Availability::Foreign) => Some(Availability::Unknown),
            Some(Availability::Unknown) => None,
        };
        self.refilter();
    }

    fn refilter(&mut self) {
        let selected = self
            .selected_installed()
            .map(|package| package.name.clone());
        self.apply_filter(selected.as_deref());
    }

    fn apply_filter(&mut self, selected: Option<&str>) {
        let query = self
            .draft
            .as_ref()
            .filter(|draft| draft.field == Field::Filter)
            .map_or(self.filter.as_str(), |draft| draft.text.as_str())
            .to_lowercase();
        self.visible_installed = self
            .installed
            .iter()
            .enumerate()
            .filter_map(|(index, package)| {
                let matches_source = self
                    .availability
                    .is_none_or(|availability| package.availability == availability);
                (matches_source
                    && (package.name.to_lowercase().contains(&query)
                        || package.version.to_lowercase().contains(&query)))
                .then_some(index)
            })
            .collect();

        let preserved = selected.and_then(|name| {
            self.installed_rows()
                .position(|package| package.name == name)
        });
        self.selections[View::Installed.index()]
            .select(preserved.or_else(|| (!self.visible_installed.is_empty()).then_some(0)));
    }

    fn refresh_inventory(&mut self) {
        self.inventory_id = self.inventory_id.wrapping_add(1);
        self.installed_state = ReadState::Loading;
        self.reads.submit(Request::Inventory(self.inventory_id));
    }

    fn invalidate_search(&mut self) {
        self.search_id = self.search_id.wrapping_add(1);
        self.reads.clear_queued_searches();
        self.search_results = Default::default();
        self.search_states = Default::default();
        self.selections[View::Search.index()].select(None);
    }

    fn refresh_search(&mut self) {
        if let Some(package) = self.selected_result() {
            self.search_selection = Some((package.source.clone(), package.name.clone()));
        }
        self.invalidate_search();
        if self.query.is_empty() {
            self.search_selection = None;
            return;
        }
        for source in SEARCH_SOURCES {
            if source == SearchSource::Aur && !self.aur_enabled() {
                continue;
            }
            self.search_states[search_index(source)] = ReadState::Loading;
            self.reads
                .submit(Request::Search(source, self.search_id, self.query.clone()));
        }
    }

    fn apply_read(&mut self, response: Response) -> bool {
        match response {
            Response::Inventory(id, result) if id == self.inventory_id => {
                match result {
                    Ok(Inventory {
                        mut packages,
                        warning,
                    }) => {
                        let selected = self
                            .selected_installed()
                            .map(|package| package.name.clone());
                        packages.sort_unstable_by(|left, right| left.name.cmp(&right.name));
                        self.marked.retain(|name| {
                            packages
                                .binary_search_by(|package| package.name.cmp(name))
                                .is_ok()
                        });
                        self.installed = packages;
                        self.installed_warning = warning;
                        self.installed_state = ReadState::Ready;
                        self.apply_filter(selected.as_deref());
                    }
                    Err(error) => {
                        self.installed_state = ReadState::Failed(error);
                        self.installed_warning = None;
                    }
                }
                true
            }
            Response::Search(source, id, result) if id == self.search_id => {
                let slot = search_index(source);
                let selected = self
                    .selected_result()
                    .map(|package| (package.source.clone(), package.name.clone()));
                match result {
                    Ok(packages) => {
                        self.search_results[slot] = packages;
                        self.search_states[slot] = ReadState::Ready;
                    }
                    Err(error) => {
                        self.search_results[slot].clear();
                        self.search_states[slot] = ReadState::Failed(error);
                    }
                }

                let position = |(source, name): &(Source, String)| {
                    self.search_rows()
                        .position(|package| &package.source == source && &package.name == name)
                };
                let preferred = self.search_selection.as_ref().and_then(position);
                let retained = preferred.or_else(|| selected.as_ref().and_then(position));
                if preferred.is_some() || !self.search_states.contains(&ReadState::Loading) {
                    self.search_selection = None;
                }
                self.selections[View::Search.index()].select(retained);
                self.clamp_selection(View::Search);
                true
            }
            Response::Fonts(id, result) if id == self.font_id => {
                if let Some(picker) = self
                    .picker
                    .as_mut()
                    .filter(|picker| picker.kind == PickerKind::Font)
                {
                    picker.load_fonts(result, self.settings.appearance.font_family.clone());
                    true
                } else {
                    false
                }
            }
            response => self.apply_source_read(response),
        }
    }

    fn edit_setting(&mut self, setting: Setting) {
        let appearance = &mut self.settings.appearance;
        let text = match setting {
            Setting::Theme => {
                self.picker = Some(Picker::themes(appearance.theme));
                return;
            }
            Setting::FontFamily => {
                self.picker = Some(Picker::fonts());
                self.font_id = self.font_id.wrapping_add(1);
                self.reads.submit(Request::Fonts(self.font_id));
                return;
            }
            Setting::Density => {
                appearance.density = match appearance.density {
                    Density::Compact => Density::Comfortable,
                    Density::Comfortable => Density::Compact,
                };
                return;
            }
            Setting::Aur => {
                self.settings.packages.aur_enabled = !self.settings.packages.aur_enabled;
                return;
            }
            Setting::Flatpak => {
                self.settings.packages.flatpak_enabled = !self.settings.packages.flatpak_enabled;
                return;
            }
            Setting::AppImages => {
                self.settings.packages.appimage_enabled = !self.settings.packages.appimage_enabled;
                return;
            }
            Setting::Save => return self.save_settings(),
            Setting::Reset => return self.reset_settings(),
            Setting::DetailsWidth => appearance.details_width_percent.to_string(),
            Setting::FontSize => appearance
                .font_size_pt
                .map(|size| size.to_string())
                .unwrap_or_default(),
        };
        self.draft = Some(Draft {
            field: Field::Setting(setting),
            text,
        });
        self.focus = Pane::List;
    }

    fn set_from_text(&mut self, setting: Setting, text: String) {
        let text = text.trim();
        match setting {
            Setting::DetailsWidth => match text.parse::<u8>() {
                Ok(width) if (25..=60).contains(&width) => {
                    self.settings.appearance.details_width_percent = width;
                }
                _ => {
                    self.notice = Some("Details width must be an integer between 25 and 60.".into())
                }
            },
            Setting::FontSize if text.is_empty() => self.settings.appearance.font_size_pt = None,
            Setting::FontSize => match text.parse::<f64>() {
                Ok(size) if (6.0..=48.0).contains(&size) => {
                    self.settings.appearance.font_size_pt = Some(size);
                }
                _ => {
                    self.notice =
                        Some("Font size must be between 6 and 48 points, or blank.".into())
                }
            },
            _ => {}
        }
    }

    fn save_settings(&mut self) {
        let Some(path) = &self.config_path else {
            self.notice =
                Some("Could not find a config directory. Set XDG_CONFIG_HOME or HOME.".into());
            return;
        };
        if let Err(error) = self.settings.save(path) {
            self.notice = Some(error);
            return;
        }

        let refresh_aur = self.config.packages.aur_enabled != self.settings.packages.aur_enabled;
        let disable_flatpak =
            self.config.packages.flatpak_enabled && !self.settings.packages.flatpak_enabled;
        let disable_images =
            self.config.packages.appimage_enabled && !self.settings.packages.appimage_enabled;
        let font_changed = self.config.appearance.font_family
            != self.settings.appearance.font_family
            || self.config.appearance.font_size_pt != self.settings.appearance.font_size_pt;
        self.config = self.settings.clone();
        if disable_flatpak {
            self.leave_flatpak_search();
            self.sources.flatpaks.invalidate();
            self.reads.clear_flatpak_inventory();
        }
        if disable_images {
            self.sources.images.invalidate();
            self.reads.clear_image_inventory();
        }
        self.notice = Some(
            if font_changed {
                "Settings saved. Font changes apply on the next --kitty launch."
            } else {
                "Settings saved."
            }
            .into(),
        );
        if refresh_aur {
            self.yay_available = packages::yay_available();
            self.refresh_search();
        }
    }

    fn reset_settings(&mut self) {
        self.settings = Config::default();
        self.notice = Some("Defaults loaded. Save to apply them.".into());
    }
}

fn switch_label(enabled: bool) -> String {
    if enabled { "On" } else { "Off" }.into()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn press(app: &mut App, keys: &str, now: Instant) {
        for character in keys.chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(character)), now);
        }
    }

    #[test]
    fn leader_opens_theme_explorer_and_quits() {
        let mut app = App::default();
        let now = Instant::now();

        press(&mut app, " ft", now);
        assert_eq!(app.view, View::Settings);
        assert_eq!(app.selection_mut().selected(), Some(0));
        press(&mut app, " e", now);
        assert_eq!(app.view, View::Installed);
        assert_eq!(app.selection_mut().selected(), None);
        press(&mut app, " q", now);
        assert!(app.should_quit);
    }

    #[test]
    fn expired_and_cancelled_sequences_do_not_trigger_actions() {
        let mut app = App::default();
        let now = Instant::now();

        press(&mut app, " f", now);
        press(&mut app, "f", now + Duration::from_millis(700));
        assert_eq!(app.view, View::Installed);

        press(&mut app, " ", now);
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        press(&mut app, "q", now);
        assert!(!app.should_quit);
        assert!(app.bindings.pending_label().is_none());
    }

    #[test]
    fn prompt_treats_normal_mode_bindings_as_text_and_restores_cancelled_edits() {
        let mut app = App::default();
        let now = Instant::now();

        press(&mut app, " ff", now);
        assert_eq!(app.view, View::Search);
        assert!(app.editing());
        press(&mut app, "jk? qé", now);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert_eq!(app.prompt(), Some(("Search packages", "jk? qé")));
        assert!(!app.editing());
        assert!(!app.show_help);
        assert!(!app.should_quit);

        press(&mut app, "/", now);
        app.handle_key(KeyEvent::from(KeyCode::Backspace), now);
        assert_eq!(app.prompt(), Some(("Search packages", "jk? q")));
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        assert_eq!(app.prompt(), Some(("Search packages", "jk? qé")));
    }

    #[test]
    fn row_navigation_clamps_to_available_items() {
        let mut app = App::default();
        let now = Instant::now();

        press(&mut app, " ft", now);
        app.update_layout(6, true);
        app.handle_key(
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            now,
        );
        assert_eq!(app.selection_mut().selected(), Some(3));
        press(&mut app, "G", now);
        assert_eq!(app.selection_mut().selected(), Some(Setting::ALL.len() - 1));
        app.navigate(Action::Down, 2);
        assert_eq!(app.selection_mut().selected(), Some(1));
        press(&mut app, "ggk", now);
        assert_eq!(app.selection_mut().selected(), Some(0));
        app.navigate(Action::Last, 0);
        assert_eq!(app.selection_mut().selected(), None);
    }

    #[test]
    fn tab_variants_cycle_views_and_resize_resets_hidden_focus() {
        let mut app = App::default();
        let now = Instant::now();

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::ALT), now);
        assert_eq!(app.view, View::Search);
        app.handle_key(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::ALT | KeyModifiers::SHIFT),
            now,
        );
        assert_eq!(app.view, View::Installed);
        app.handle_key(KeyEvent::from(KeyCode::BackTab), now);
        assert_eq!(app.view, View::Settings);

        app.update_layout(10, true);
        press(&mut app, "l", now);
        assert_eq!(app.focus, Pane::Details);
        app.update_layout(10, false);
        assert_eq!(app.focus, Pane::List);
        press(&mut app, "l", now);
        assert_eq!(app.focus, Pane::List);
    }

    fn installed_package(
        name: &str,
        version: &str,
        availability: Availability,
    ) -> InstalledPackage {
        InstalledPackage {
            name: name.into(),
            version: version.into(),
            availability,
        }
    }

    fn search_result(name: &str, source: Source) -> SearchResult {
        SearchResult {
            name: name.into(),
            version: "2.0-1".into(),
            source,
            description: String::new(),
        }
    }

    #[test]
    fn installed_filter_combines_live_text_and_availability() {
        let mut app = App::default();
        app.apply_read(Response::Inventory(
            0,
            Ok(Inventory {
                packages: vec![
                    installed_package("ripgrep-git", "2.0.r1", Availability::Foreign),
                    installed_package("ripgrep", "2.0", Availability::Repository),
                    installed_package("fd", "1.4", Availability::Repository),
                ],
                warning: None,
            }),
        ));
        let now = Instant::now();

        press(&mut app, "/RIPGREP", now);
        assert_eq!(app.installed_rows().count(), 2);
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        assert_eq!(app.installed_rows().count(), 3);

        press(&mut app, "aa/2.0", now);
        assert_eq!(app.installed_rows().count(), 1);
        assert_eq!(app.selected_installed().unwrap().name, "ripgrep-git");
        assert_eq!(app.installed_version("ripgrep"), Some("2.0"));
    }

    #[test]
    fn search_keeps_repository_results_when_aur_fails_and_discards_stale_response() {
        let mut app = App {
            search_id: 2,
            ..App::default()
        };
        let repo = search_result("archoxide", Source::Repository("extra".into()));
        let stale = search_result("unrelated", Source::Aur);

        assert!(app.apply_read(Response::Search(
            SearchSource::Repository,
            2,
            Ok(vec![repo.clone()])
        )));
        assert!(!app.apply_read(Response::Search(SearchSource::Aur, 1, Ok(vec![stale]))));
        assert!(app.apply_read(Response::Search(
            SearchSource::Aur,
            2,
            Err("AUR unavailable".into())
        )));

        assert_eq!(app.search_rows().collect::<Vec<_>>(), [&repo]);
        assert_eq!(
            app.search_states[1],
            ReadState::Failed("AUR unavailable".into())
        );
    }

    #[test]
    fn search_preserves_the_selected_source_when_repository_rows_arrive_later() {
        let mut app = App {
            search_states: [ReadState::Loading, ReadState::Loading],
            ..App::default()
        };
        let repo = search_result("same-name", Source::Repository("extra".into()));
        let aur = search_result("same-name", Source::Aur);

        app.apply_read(Response::Search(
            SearchSource::Aur,
            0,
            Ok(vec![aur.clone()]),
        ));
        assert_eq!(app.selected_result(), Some(&aur));

        app.apply_read(Response::Search(
            SearchSource::Repository,
            0,
            Ok(vec![repo.clone()]),
        ));
        assert_eq!(app.search_rows().collect::<Vec<_>>(), [&repo, &aur]);
        assert_eq!(app.selected_result(), Some(&aur));
    }

    #[test]
    fn package_actions_require_confirmation_and_keep_the_selected_target() {
        let mut app = App::default();
        app.apply_read(Response::Inventory(
            0,
            Ok(Inventory {
                packages: vec![installed_package(
                    "ripgrep",
                    "2.0",
                    Availability::Repository,
                )],
                warning: None,
            }),
        ));
        let now = Instant::now();

        press(&mut app, "d", now);
        assert!(app.take_transaction().is_none());
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        assert!(app.take_transaction().is_none());

        press(&mut app, "d", now);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert!(
            matches!(app.take_transaction(), Some(Transaction::Remove(package)) if package.name == "ripgrep")
        );

        app.yay_available = false;
        press(&mut app, " u", now);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert!(matches!(
            app.take_transaction(),
            Some(Transaction::Upgrade { aur: false })
        ));
    }

    #[test]
    fn scoped_upgrades_use_marks_across_filters_or_the_highlighted_row() {
        let mut app = App::default();
        let inventory = vec![
            installed_package("alpha", "1", Availability::Repository),
            installed_package("beta", "1", Availability::Foreign),
            installed_package("gamma", "1", Availability::Repository),
        ];
        app.apply_read(Response::Inventory(
            0,
            Ok(Inventory {
                packages: inventory,
                warning: None,
            }),
        ));
        let now = Instant::now();

        press(&mut app, "u", now);
        assert!(
            matches!(&app.confirmation, Some(Transaction::UpgradeSelected { aur: false, names }) if names == &["alpha"])
        );
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        assert!(app.take_transaction().is_none());

        press(&mut app, "xjx/gamma", now);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert_eq!(app.marked_count(), 2);
        press(&mut app, "u", now);
        assert!(app.confirmation.is_none());
        assert!(app.notice.as_deref().unwrap().contains("requires yay"));
        app.yay_available = true;
        press(&mut app, "u", now);
        assert!(
            matches!(&app.confirmation, Some(Transaction::UpgradeSelected { aur: true, names }) if names == &["alpha", "beta"])
        );
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);

        press(&mut app, "U", now);
        assert!(
            matches!(&app.confirmation, Some(Transaction::UpgradeExcept { aur: true, names }) if names == &["alpha", "beta"])
        );
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        press(&mut app, " u", now);
        assert!(matches!(
            &app.confirmation,
            Some(Transaction::Upgrade { aur: true })
        ));
        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        press(&mut app, "XU", now);
        assert!(
            matches!(&app.confirmation, Some(Transaction::UpgradeExcept { aur: true, names }) if names == &["gamma"])
        );
    }

    #[test]
    fn settings_are_applied_only_after_save_and_reset_is_a_draft() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("archoxide/config.toml");
        let mut app = App::new(Config::default(), Some(path.clone()), None);
        let now = Instant::now();

        press(&mut app, " ft", now);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        press(&mut app, "j", now);
        assert_eq!(app.rendered_theme(), Theme::Light);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert_eq!(app.setting_value(Setting::Theme), "Light");
        assert_eq!(app.config.appearance.theme, Theme::Dark);
        assert!(!path.exists());

        press(&mut app, "s", now);
        assert_eq!(app.config.appearance.theme, Theme::Light);
        assert_eq!(Config::load(&path).unwrap(), app.config);
        press(&mut app, "r", now);
        assert_eq!(app.setting_value(Setting::Theme), "Dark");
        assert_eq!(Config::load(&path).unwrap().appearance.theme, Theme::Light);
    }

    #[test]
    fn searchable_font_picker_saves_a_family_from_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut app = App::new(Config::default(), Some(path.clone()), None);
        app.view = View::Settings;
        app.picker = Some(Picker::fonts());
        assert!(app.apply_read(Response::Fonts(
            0,
            Ok(vec!["Adwaita Sans".into(), "DejaVu Sans Mono".into()])
        )));
        let now = Instant::now();

        press(&mut app, "/dejavu", now);
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert_eq!(app.setting_value(Setting::FontFamily), "DejaVu Sans Mono");
        assert!(app.config.appearance.font_family.is_none());
        press(&mut app, "s", now);
        assert_eq!(
            Config::load(&path)
                .unwrap()
                .appearance
                .font_family
                .as_deref(),
            Some("DejaVu Sans Mono")
        );
    }

    #[test]
    fn disabled_sources_are_persisted_and_skipped_by_navigation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut app = App::new(Config::default(), Some(path.clone()), None);
        let now = Instant::now();

        press(&mut app, " ftjjjjjj", now);
        assert_eq!(app.selected_setting(), Some(Setting::Flatpak));
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        press(&mut app, "j", now);
        assert_eq!(app.selected_setting(), Some(Setting::AppImages));
        app.handle_key(KeyEvent::from(KeyCode::Enter), now);
        assert_eq!(app.setting_value(Setting::Flatpak), "Off");
        assert_eq!(app.setting_value(Setting::AppImages), "Off");
        assert!(app.views().any(|view| view == View::Flatpak));

        app.sources.image_busy = true;
        press(&mut app, "s", now);
        let loaded = Config::load(&path).unwrap();
        assert!(!loaded.packages.flatpak_enabled);
        assert!(!loaded.packages.appimage_enabled);
        assert!(loaded.packages.aur_enabled);
        assert_eq!(
            app.views().collect::<Vec<_>>(),
            [View::Installed, View::Search, View::Settings]
        );
        assert!(!app.apply_read(Response::FlatpakInventory(0, Ok(Vec::new()))));
        assert!(!app.apply_read(Response::ImageInventory(0, Ok(Vec::new()))));
        assert!(app.apply_read(Response::ImageOperation(Ok("Done".into()))));
        assert!(!app.has_pending_reads());

        press(&mut app, " p a", now);
        assert_eq!(app.view, View::Settings);
        assert!(
            app.notice
                .as_deref()
                .unwrap()
                .contains("AppImages support is off")
        );
        app.handle_key(KeyEvent::from(KeyCode::BackTab), now);
        assert_eq!(app.view, View::Search);
        app.handle_key(KeyEvent::from(KeyCode::Tab), now);
        assert_eq!(app.view, View::Settings);

        press(&mut app, "rs", now);
        assert_eq!(app.views().collect::<Vec<_>>(), View::ALL);
    }
}
