use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(700);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    First,
    Last,
    HalfPageUp,
    HalfPageDown,
    FocusList,
    FocusDetails,
    NextView,
    PreviousView,
    Installed,
    Search,
    Theme,
    Flatpak,
    AppImages,
    Apps,
    SearchFlatpak,
    ReplaceAppImage,
    Prompt,
    CycleAvailability,
    RefreshInventory,
    Install,
    Remove,
    Upgrade,
    UpgradeSelected,
    UpgradeExcept,
    ToggleMark,
    ClearMarks,
    Activate,
    SaveSettings,
    ResetSettings,
    Help,
    Quit,
}

#[derive(Clone, Copy, Debug)]
enum Prefix {
    Go,
    Leader,
    Find,
}

impl Prefix {
    fn label(self) -> &'static str {
        match self {
            Self::Go => "g",
            Self::Leader => "Space",
            Self::Find => "Space f",
        }
    }
}

#[derive(Debug, Default)]
pub struct Bindings {
    pending: Option<(Prefix, Instant)>,
}

impl Bindings {
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.pending
            .map(|(_, deadline)| deadline.saturating_duration_since(now))
    }

    pub fn expire(&mut self, now: Instant) {
        if self.remaining(now).is_some_and(|time| time.is_zero()) {
            self.pending = None;
        }
    }

    pub fn pending_label(&self) -> Option<&'static str> {
        self.pending.map(|(prefix, _)| prefix.label())
    }

    pub fn resolve(&mut self, key: KeyEvent, now: Instant) -> Option<Action> {
        use Action::*;
        use KeyCode::{BackTab, Char, Tab};

        self.expire(now);
        let pending = self.pending.take().map(|(prefix, _)| prefix);
        if plain_key(key) {
            match (pending, key.code) {
                (Some(Prefix::Go), Char('g')) => return Some(First),
                (Some(Prefix::Leader), Char('e')) => return Some(Installed),
                (Some(Prefix::Leader), Char('p')) => return Some(Flatpak),
                (Some(Prefix::Leader), Char('a')) => return Some(AppImages),
                (Some(Prefix::Leader), Char('o')) => return Some(Apps),
                (Some(Prefix::Leader), Char('f')) => {
                    self.pending = Some((Prefix::Find, now + SEQUENCE_TIMEOUT));
                    return None;
                }
                (Some(Prefix::Leader), Char('q')) => return Some(Quit),
                (Some(Prefix::Leader), Char('u')) => return Some(Upgrade),
                (Some(Prefix::Find), Char('f')) => return Some(Search),
                (Some(Prefix::Find), Char('t')) => return Some(Theme),
                _ => {}
            }
        }

        if key.modifiers == KeyModifiers::CONTROL {
            return match key.code {
                KeyCode::Home => Some(First),
                KeyCode::End => Some(Last),
                Char('u') => Some(HalfPageUp),
                Char('d') => Some(HalfPageDown),
                Char('h') => Some(FocusList),
                Char('l') => Some(FocusDetails),
                _ => None,
            };
        }

        if matches!(key.code, Tab | BackTab)
            && (key.modifiers - (KeyModifiers::ALT | KeyModifiers::SHIFT)).is_empty()
        {
            return Some(
                if key.code == BackTab || key.modifiers.contains(KeyModifiers::SHIFT) {
                    PreviousView
                } else {
                    NextView
                },
            );
        }

        if !plain_key(key) {
            return None;
        }

        match key.code {
            Char('k') | KeyCode::Up => Some(Up),
            Char('j') | KeyCode::Down => Some(Down),
            Char('G') => Some(Last),
            Char('h') => Some(FocusList),
            Char('l') => Some(FocusDetails),
            Char('/') => Some(Prompt),
            Char('a') => Some(CycleAvailability),
            Char('R') => Some(RefreshInventory),
            Char('i') => Some(Install),
            Char('d') => Some(Remove),
            Char('f') => Some(SearchFlatpak),
            Char('e') => Some(ReplaceAppImage),
            Char('u') => Some(UpgradeSelected),
            Char('U') => Some(UpgradeExcept),
            Char('x') => Some(ToggleMark),
            Char('X') => Some(ClearMarks),
            Char('s') => Some(SaveSettings),
            Char('r') => Some(ResetSettings),
            KeyCode::Enter => Some(Activate),
            Char('?') => Some(Help),
            Char('g') => {
                self.pending = Some((Prefix::Go, now + SEQUENCE_TIMEOUT));
                None
            }
            Char(' ') => {
                self.pending = Some((Prefix::Leader, now + SEQUENCE_TIMEOUT));
                None
            }
            _ => None,
        }
    }
}

pub fn plain_key(key: KeyEvent) -> bool {
    (key.modifiers - KeyModifiers::SHIFT).is_empty()
}
