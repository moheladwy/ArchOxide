use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

use crate::input::plain_key;

#[derive(Debug, Default)]
pub struct TextInput {
    text: String,
    cursor: usize,
}

pub enum Edit {
    Moved,
    Changed,
}

impl From<String> for TextInput {
    fn from(text: String) -> Self {
        Self {
            cursor: text.len(),
            text,
        }
    }
}

impl TextInput {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn into_string(self) -> String {
        self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn edit(&mut self, key: KeyEvent) -> Option<Edit> {
        use KeyCode::*;

        if key.modifiers == KeyModifiers::CONTROL {
            self.cursor = match key.code {
                Char('u') => {
                    self.text.clear();
                    self.cursor = 0;
                    return Some(Edit::Changed);
                }
                Left => self
                    .text
                    .unicode_word_indices()
                    .rev()
                    .find(|&(start, _)| start < self.cursor)
                    .map_or(0, |(start, _)| start),
                Right => self
                    .text
                    .unicode_word_indices()
                    .find(|&(start, _)| start > self.cursor)
                    .map_or(self.text.len(), |(start, _)| start),
                _ => return None,
            };
            return Some(Edit::Moved);
        }
        if !plain_key(key) {
            return None;
        }

        self.cursor = match key.code {
            Home => 0,
            End => self.text.len(),
            Left => self.previous_boundary(),
            Right => self.boundary_at_or_after(self.cursor.saturating_add(1)),
            Backspace if self.cursor > 0 => {
                let start = self.previous_boundary();
                self.text.replace_range(start..self.cursor, "");
                self.cursor = self.boundary_at_or_after(start);
                return Some(Edit::Changed);
            }
            Delete if self.cursor < self.text.len() => {
                let end = self.boundary_at_or_after(self.cursor + 1);
                self.text.replace_range(self.cursor..end, "");
                self.cursor = self.boundary_at_or_after(self.cursor);
                return Some(Edit::Changed);
            }
            Char(character) if !character.is_control() => {
                self.text.insert(self.cursor, character);
                self.cursor = self.boundary_at_or_after(self.cursor + character.len_utf8());
                return Some(Edit::Changed);
            }
            _ => return None,
        };
        Some(Edit::Moved)
    }

    fn previous_boundary(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(start, _)| start)
    }

    fn boundary_at_or_after(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find(|&(start, _)| start >= offset)
            .map_or(self.text.len(), |(start, _)| start)
    }
}

#[cfg(test)]
#[path = "../tests/unit/text_input.rs"]
mod tests;
