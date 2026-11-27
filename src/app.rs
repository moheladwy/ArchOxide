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
    text_input::{Edit, TextInput},
};

mod desktop_actions;
mod package_details;
mod source_actions;

use desktop_actions::DesktopApps;
pub use package_details::Info as PackageInfo;
use package_details::PackageDetails;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Installed,
    Search,
    Flatpak,
    AppImages,
    Apps,
    Settings,
}

impl View {
    pub const ALL: [Self; 6] = [
        Self::Installed,
        Self::Search,
        Self::Flatpak,
        Self::AppImages,
        Self::Apps,
        Self::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::Search => "Search",
            Self::Flatpak => "Flatpak",
            Self::AppImages => "AppImages",
            Self::Apps => "Apps",
            Self::Settings => "Settings",
        }
    }

    pub fn has_package_details(self) -> bool {
        matches!(
            self,
            Self::Installed | Self::Search | Self::Flatpak | Self::AppImages
        )
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
    DesktopAppFilter,
    ImageImport,
    ImageReplace,
    Setting(Setting),
}

#[derive(Debug)]
struct Draft {
    field: Field,
    input: TextInput,
}

impl Draft {
    fn new(field: Field, text: String) -> Self {
        Self {
            field,
            input: text.into(),
        }
    }
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
    pub desktop_apps: DesktopApps,
    pub package_details: PackageDetails,
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
    search_results: [Vec<SearchResult>; SEARCH_SOURCES.len()],
    search_order: Vec<(usize, usize)>,
    pub search_states: [ReadState; SEARCH_SOURCES.len()],
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
        self.sync_package_details(false);
        match transaction {
            Transaction::Flatpak(_) => self.refresh_flatpak_inventory(),
            Transaction::AppImage(_) => self.refresh_image_inventory(),
            _ => self.refresh_inventory(),
        }
        if self.desktop_apps.state != ReadState::Idle {
            self.refresh_desktop_apps();
        }
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
        self.search_order
            .iter()
            .map(|&(source, row)| &self.search_results[source][row])
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
                Field::DesktopAppFilter => "Filter desktop apps",
                Field::ImageImport => "Import AppImage from a path or HTTPS URL",
                Field::ImageReplace => "Replace AppImage from a path or HTTPS URL",
                Field::Setting(Setting::DetailsWidth) => "Details width (%)",
                Field::Setting(Setting::FontSize) => "Font size (pt)",
                Field::Setting(setting) => setting.label(),
            };
            return Some((title, draft.input.as_str()));
        }
        match self.view {
            View::Installed => Some(("Filter installed", &self.filter)),
            View::Search => Some(("Search packages", &self.query)),
            View::Flatpak => match &self.sources.flatpak_query {
                Some(query) => Some(("Search Flatpak remotes", query)),
                None => Some(("Filter installed Flatpaks", &self.sources.flatpaks.filter)),
            },
            View::AppImages => Some(("Filter managed AppImages", &self.sources.images.filter)),
            View::Apps => Some(("Filter desktop apps", &self.desktop_apps.filter)),
            View::Settings => None,
        }
    }

    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }

    pub fn prompt_cursor(&self) -> Option<usize> {
        self.draft.as_ref().map(|draft| draft.input.cursor())
    }

    pub fn update_layout(&mut self, visible_rows: usize, details_visible: bool) {
        self.visible_rows = visible_rows;
        self.details_visible = details_visible;
        if !details_visible {
            self.focus = Pane::List;
        }
        self.clamp_selection(self.view);
        self.sync_package_details(details_visible);
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) {
        if key.kind == KeyEventKind::Release
            || (self.view == View::Apps && !self.desktop_apps.accept_key(key, now))
        {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            self.should_quit = true;
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
                        self.transaction = self.confirmation.take();
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
            if self.focus == Pane::Details && self.view.has_package_details() {
                self.focus = Pane::List;
                return;
            }
            if self.view == View::Flatpak && self.sources.flatpak_query.is_some() {
                self.leave_flatpak_search();
                return;
            }
        }
        if let Some(action) = self.bindings.resolve(key, now)
            && (key.kind == KeyEventKind::Press
                || !matches!(
                    (action, self.view),
                    (Action::ToggleMark, _) | (Action::Activate, View::Apps)
                ))
        {
            self.apply_action(action);
        }
    }

    fn edit_text(&mut self, key: KeyEvent) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc if plain_key(key) => {
                let filtering = draft.field == Field::Filter;
                self.draft = None;
                if filtering {
                    self.refilter();
                }
            }
            KeyCode::Enter if plain_key(key) && key.kind == KeyEventKind::Press => {
                let draft = self.draft.take().expect("an active text prompt");
                let text = draft.input.into_string();
                match draft.field {
                    Field::Filter => {
                        self.filter = text;
                        self.refilter();
                    }
                    Field::Search => {
                        self.query = text.trim().to_owned();
                        self.search_selection = None;
                        self.refresh_search();
                    }
                    Field::FlatpakFilter => self.sources.flatpaks.filter = text,
                    Field::FlatpakSearch => self.search_flatpaks(text),
                    Field::ImageFilter => self.sources.images.filter = text,
                    Field::DesktopAppFilter => self.desktop_apps.filter = text,
                    Field::ImageImport | Field::ImageReplace => {
                        self.confirm_image_source(draft.field, text)
                    }
                    Field::Setting(setting) => self.set_from_text(setting, text),
                }
            }
            _ => {
                if matches!(draft.input.edit(key), Some(Edit::Changed))
                    && draft.field == Field::Filter
                {
                    self.refilter();
                }
            }
        }
    }

    fn apply_action(&mut self, action: Action) {
        if action == Action::RefreshInventory {
            self.sync_package_details(false);
        }
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            Action::NextView => self.cycle_view(1),
            Action::PreviousView => self.cycle_view(View::ALL.len() - 1),
            Action::Installed => self.open(View::Installed),
            Action::Flatpak => self.open(View::Flatpak),
            Action::AppImages => self.open(View::AppImages),
            Action::Apps => self.open(View::Apps),
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
            Action::FocusDetails if self.details_visible || self.view.has_package_details() => {
                self.focus = Pane::Details;
            }
            Action::CycleAvailability if self.view == View::Installed => self.cycle_availability(),
            Action::RefreshInventory if self.view == View::Installed => self.refresh_inventory(),
            Action::RefreshInventory if self.view == View::Apps => self.refresh_desktop_apps(),
            Action::Install if self.view == View::Search => {
                if let Some(result) = self.selected_result() {
                    match result.install_transaction() {
                        Ok(transaction) => self.confirmation = Some(transaction),
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            Action::Remove
                if self.view == View::Installed && self.installed_state == ReadState::Ready =>
            {
                let names: Vec<_> = self
                    .installed_targets()
                    .into_iter()
                    .map(|package| package.name.clone())
                    .collect();
                if names.is_empty() {
                    self.notice = Some("Select at least one installed package.".into());
                } else {
                    self.confirmation = Some(Transaction::Remove(names));
                }
            }
            Action::Upgrade
                if matches!(self.view, View::Installed | View::Search | View::Settings) =>
            {
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
            Action::Activate if self.view == View::Apps => self.launch_selected_desktop_app(),
            Action::Activate if self.view == View::Settings => {
                if let Some(setting) = self.selected_setting() {
                    self.edit_setting(setting);
                }
            }
            Action::SaveSettings if self.view == View::Settings => self.save_settings(),
            Action::ResetSettings if self.view == View::Settings => self.reset_settings(),
            _ => {
                if !self.apply_source_action(action) {
                    match self.focus {
                        Pane::List => self.navigate(action, self.row_count(self.view)),
                        Pane::Details if self.view.has_package_details() => {
                            self.scroll_package_details(action);
                        }
                        Pane::Details => {}
                    }
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
        if view == View::Apps && self.desktop_apps.state == ReadState::Idle {
            self.refresh_desktop_apps();
        }
        if view == View::Search
            && !self.search_terms().1.is_empty()
            && self
                .search_states
                .iter()
                .all(|state| *state == ReadState::Idle)
        {
            self.refresh_search();
        }
    }

    fn installed_targets(&self) -> Vec<&InstalledPackage> {
        if self.marked.is_empty() {
            self.selected_installed().into_iter().collect()
        } else {
            self.installed
                .iter()
                .filter(|package| self.is_marked(&package.name))
                .collect()
        }
    }

    fn confirm_scoped_upgrade(&mut self, exclude: bool) {
        if self.installed_state != ReadState::Ready {
            self.notice = Some("Refresh Installed before choosing packages to update.".into());
            return;
        }
        let selected = self.installed_targets();
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
            View::Apps => (Field::DesktopAppFilter, self.desktop_apps.filter.clone()),
            View::Settings => return,
        };
        self.draft = Some(Draft::new(field, text));
        self.focus = Pane::List;
    }

    fn row_count(&self, view: View) -> usize {
        match view {
            View::Installed => self.visible_installed.len(),
            View::Search => self.search_order.len(),
            View::Flatpak if self.sources.flatpak_query.is_some() => {
                self.sources.flatpak_results.len()
            }
            View::Flatpak => self.flatpak_rows().count(),
            View::AppImages => self.image_rows().count(),
            View::Apps => self.desktop_app_rows().count(),
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
            self.search_selection = self
                .selected_result()
                .map(|package| (package.source.clone(), package.name.clone()));
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
            .map_or(self.filter.as_str(), |draft| draft.input.as_str())
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
        if self.view == View::Search {
            self.sync_package_details(false);
        }
        self.search_id = self.search_id.wrapping_add(1);
        self.reads.clear_queued_searches();
        self.search_results = Default::default();
        self.search_order.clear();
        self.search_states = Default::default();
        self.selections[View::Search.index()].select(None);
    }

    pub fn search_terms(&self) -> (Option<SearchSource>, &str) {
        let query = self.query.trim();
        let Some((prefix, term)) = query.split_once('/') else {
            return (None, query);
        };
        let source = match prefix {
            "pacman" => SearchSource::Repository,
            "aur" => SearchSource::Aur,
            "flatpak" => SearchSource::Flatpak,
            _ => return (None, query),
        };
        (Some(source), term.trim())
    }

    fn search_unavailable(&self, source: SearchSource) -> Option<&'static str> {
        match source {
            SearchSource::Aur if !self.config.packages.aur_enabled => {
                Some("Enable AUR support in Settings.")
            }
            SearchSource::Aur if !self.yay_available => Some("yay was not found."),
            SearchSource::Flatpak if !self.config.packages.flatpak_enabled => {
                Some("Enable Flatpak support in Settings.")
            }
            SearchSource::AppImage if !self.config.packages.appimage_enabled => {
                Some("Enable AppImages support in Settings.")
            }
            _ => None,
        }
    }

    fn refresh_search(&mut self) {
        self.invalidate_search();
        let (scope, term) = self.search_terms();
        if term.is_empty() {
            self.search_selection = None;
            return;
        }
        let term = term.to_owned();
        for source in SEARCH_SOURCES {
            if scope.is_some_and(|scope| scope != source) {
                continue;
            }
            let slot = search_index(source);
            if let Some(message) = self.search_unavailable(source) {
                if scope.is_some() {
                    self.search_states[slot] = ReadState::Failed(message.into());
                }
                continue;
            }
            self.search_states[slot] = ReadState::Loading;
            self.reads
                .submit(Request::Search(source, self.search_id, term.clone()));
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

                let query = self.search_terms().1.to_lowercase();
                let mut order: Vec<_> = self
                    .search_results
                    .iter()
                    .enumerate()
                    .flat_map(|(source, rows)| (0..rows.len()).map(move |row| (source, row)))
                    .collect();
                // Stable sorting keeps source and backend order for equally relevant matches.
                order.sort_by_cached_key(|&(source, row)| {
                    self.search_results[source][row].match_rank(&query)
                });
                self.search_order = order;

                let retained = self.search_selection.as_ref().and_then(|(source, name)| {
                    self.search_rows()
                        .position(|package| &package.source == source && &package.name == name)
                });
                if retained.is_none() && !self.search_states.contains(&ReadState::Loading) {
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
            Response::DesktopApps(id, result) if id == self.desktop_apps.revision => {
                self.load_desktop_apps(result);
                true
            }
            Response::LaunchDesktopApp(name, result) => {
                self.desktop_apps.launching = false;
                self.notice = Some(match result {
                    Ok(()) => format!("Opened {name}."),
                    Err(error) => error,
                });
                true
            }
            Response::PackageInfo(id, result) if id == self.package_details.revision => {
                self.package_details.info = Some(match result {
                    Ok(info) => PackageInfo::Ready(info),
                    Err(error) => PackageInfo::Failed(error),
                });
                true
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
        self.draft = Some(Draft::new(Field::Setting(setting), text));
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
        let refresh_search = refresh_aur
            || self.config.packages.flatpak_enabled != self.settings.packages.flatpak_enabled
            || self.config.packages.appimage_enabled != self.settings.packages.appimage_enabled;
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
        }
        if refresh_search {
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
#[path = "../tests/unit/app.rs"]
mod tests;
