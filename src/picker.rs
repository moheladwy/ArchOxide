use ratatui::{
    crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    widgets::ListState,
};

use crate::{config::Theme, input::plain_key};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Theme,
    Font,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Theme(Theme),
    Font(Option<String>),
}

impl Choice {
    pub fn label(&self) -> &str {
        match self {
            Self::Theme(theme) => theme.label(),
            Self::Font(family) => family.as_deref().unwrap_or("Inherit terminal font"),
        }
    }
}

pub enum Outcome {
    Choose(Choice),
    Close,
}

#[derive(Debug)]
pub struct Picker {
    pub kind: Kind,
    pub query: String,
    pub editing: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub selection: ListState,
    items: Vec<Choice>,
    visible: Vec<usize>,
}

impl Picker {
    pub fn themes(current: Theme) -> Self {
        let items = Theme::ALL.iter().copied().map(Choice::Theme).collect();
        Self::new(Kind::Theme, items, Choice::Theme(current))
    }

    pub fn fonts() -> Self {
        let mut picker = Self::new(Kind::Font, vec![Choice::Font(None)], Choice::Font(None));
        picker.loading = true;
        picker
    }

    fn new(kind: Kind, items: Vec<Choice>, current: Choice) -> Self {
        let mut picker = Self {
            kind,
            items,
            query: String::new(),
            editing: false,
            loading: false,
            error: None,
            selection: ListState::default(),
            visible: Vec::new(),
        };
        picker.refilter(Some(&current));
        picker
    }

    pub fn load_fonts(&mut self, result: Result<Vec<String>, String>, current: Option<String>) {
        self.loading = false;
        match result {
            Ok(families) => {
                self.items
                    .extend(families.into_iter().map(|name| Choice::Font(Some(name))));
                self.refilter(Some(&Choice::Font(current)));
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn visible(&self) -> impl Iterator<Item = &Choice> + '_ {
        self.visible.iter().map(|&index| &self.items[index])
    }

    pub fn selected(&self) -> Option<&Choice> {
        let row = self.selection.selected()?;
        self.visible.get(row).map(|&index| &self.items[index])
    }

    fn refilter(&mut self, current: Option<&Choice>) {
        let query = self.query.to_lowercase();
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item.label()
                    .to_lowercase()
                    .contains(&query)
                    .then_some(index)
            })
            .collect();
        let row = current.and_then(|choice| self.visible().position(|item| item == choice));
        self.selection
            .select(row.or_else(|| (!self.visible.is_empty()).then_some(0)));
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Outcome> {
        if self.editing && key.code == KeyCode::Char('u') && key.modifiers == KeyModifiers::CONTROL
        {
            self.query.clear();
            self.refilter(None);
            return None;
        }
        let jump = key.modifiers == KeyModifiers::CONTROL
            && matches!(key.code, KeyCode::Home | KeyCode::End);
        if !(plain_key(key) || jump) {
            return None;
        }
        match key.code {
            KeyCode::Esc if self.editing => self.editing = false,
            KeyCode::Esc => return Some(Outcome::Close),
            KeyCode::Enter if key.kind == KeyEventKind::Press => {
                return self.selected().cloned().map(Outcome::Choose);
            }
            KeyCode::Backspace if self.editing => {
                self.query.pop();
                self.refilter(None);
            }
            KeyCode::Char(character) if self.editing && !character.is_control() => {
                self.query.push(character);
                self.refilter(None);
            }
            KeyCode::Char('/') => self.editing = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::Home => self.move_by(isize::MIN),
            KeyCode::End | KeyCode::Char('G') => self.move_by(isize::MAX),
            _ => {}
        }
        None
    }

    fn move_by(&mut self, delta: isize) {
        if let Some(row) = self.selection.selected() {
            self.selection.select(Some(
                row.saturating_add_signed(delta).min(self.visible.len() - 1),
            ));
        }
    }
}
