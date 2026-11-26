use std::time::Instant;

use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::ListState,
};

use crate::input::{Action, Bindings, plain_key};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Installed,
    Search,
    Settings,
}

impl View {
    pub const ALL: [Self; 3] = [Self::Installed, Self::Search, Self::Settings];

    pub fn title(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::Search => "Search",
            Self::Settings => "Settings",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn row_count(self) -> usize {
        match self {
            Self::Installed | Self::Search => 0,
            Self::Settings => SETTINGS.len(),
        }
    }
}

pub const SETTINGS: [(&str, &str); 6] = [
    ("Theme", "Color palette for the terminal interface."),
    ("UI density", "Padding and spacing between rows."),
    ("Details width", "How much space the details pane uses."),
    (
        "Font family",
        "Monospace font for the dedicated Kitty window.",
    ),
    ("Font size", "Font size in points for the Kitty window."),
    ("AUR support", "Whether to use yay for AUR packages."),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    List,
    Details,
}

#[derive(Debug, Default)]
pub struct App {
    pub view: View,
    pub selection: ListState,
    pub focus: Pane,
    pub bindings: Bindings,
    pub show_help: bool,
    pub help_scroll: u16,
    pub should_quit: bool,
    filter: String,
    query: String,
    draft: Option<String>,
    details_visible: bool,
    visible_rows: usize,
}

impl App {
    pub fn prompt(&self) -> Option<(&'static str, &str)> {
        let (title, committed) = match self.view {
            View::Installed => ("Filter installed", &self.filter),
            View::Search => ("Search packages", &self.query),
            View::Settings => return None,
        };
        Some((title, self.draft.as_deref().unwrap_or(committed)))
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
        self.clamp_selection(self.view.row_count());
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            self.should_quit = true;
            return;
        }
        if self.show_help {
            if plain_key(key) {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('?') => self.show_help = false,
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.help_scroll = self.help_scroll.saturating_add(1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.help_scroll = self.help_scroll.saturating_sub(1);
                    }
                    _ => {}
                }
            }
            return;
        }
        if let Some(draft) = &mut self.draft {
            if plain_key(key) {
                match key.code {
                    KeyCode::Esc => self.draft = None,
                    KeyCode::Enter => {
                        let value = std::mem::take(draft);
                        self.draft = None;
                        match self.view {
                            View::Installed => self.filter = value,
                            View::Search => self.query = value,
                            View::Settings => {}
                        }
                    }
                    KeyCode::Backspace => {
                        draft.pop();
                    }
                    KeyCode::Char(character) if !character.is_control() => draft.push(character),
                    _ => {}
                }
            }
            return;
        }

        if let Some(action) = self.bindings.resolve(key, now) {
            self.apply(action);
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            Action::NextView => self.cycle_view(1),
            Action::PreviousView => self.cycle_view(View::ALL.len() - 1),
            Action::Installed => self.open(View::Installed),
            Action::Search => {
                self.open(View::Search);
                self.open_prompt();
            }
            Action::Theme => {
                self.open(View::Settings);
                self.selection.select_first();
            }
            Action::Prompt => self.open_prompt(),
            Action::FocusList => self.focus = Pane::List,
            Action::FocusDetails if self.details_visible => self.focus = Pane::Details,
            Action::FocusDetails => {}
            _ if self.focus == Pane::List => self.navigate(action, self.view.row_count()),
            _ => {}
        }
    }

    fn open(&mut self, view: View) {
        if view != self.view {
            self.view = view;
            self.selection = ListState::default();
        }
        self.focus = Pane::List;
        self.clamp_selection(view.row_count());
    }

    fn cycle_view(&mut self, step: usize) {
        self.open(View::ALL[(self.view.index() + step) % View::ALL.len()]);
    }

    fn open_prompt(&mut self) {
        if let Some((_, text)) = self.prompt() {
            self.draft = Some(text.to_owned());
            self.focus = Pane::List;
        }
    }

    fn clamp_selection(&mut self, row_count: usize) {
        let last = row_count.checked_sub(1);
        self.selection
            .select(last.map(|last| self.selection.selected().unwrap_or(0).min(last)));
    }

    fn navigate(&mut self, action: Action, row_count: usize) {
        self.clamp_selection(row_count);
        let Some(row) = self.selection.selected() else {
            return;
        };
        let half_page = (self.visible_rows / 2).max(1);
        let next = match action {
            Action::Up => row.saturating_sub(1),
            Action::Down => row + 1,
            Action::First => 0,
            Action::Last => row_count - 1,
            Action::HalfPageUp => row.saturating_sub(half_page),
            Action::HalfPageDown => row + half_page,
            _ => return,
        };
        self.selection.select(Some(next.min(row_count - 1)));
    }
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
        assert_eq!(app.selection.selected(), Some(0));
        press(&mut app, " e", now);
        assert_eq!(app.view, View::Installed);
        assert_eq!(app.selection.selected(), None);
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
        assert_eq!(app.selection.selected(), Some(3));
        press(&mut app, "G", now);
        assert_eq!(app.selection.selected(), Some(5));
        app.navigate(Action::Down, 2);
        assert_eq!(app.selection.selected(), Some(1));
        press(&mut app, "ggk", now);
        assert_eq!(app.selection.selected(), Some(0));
        app.navigate(Action::Last, 0);
        assert_eq!(app.selection.selected(), None);
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
}
