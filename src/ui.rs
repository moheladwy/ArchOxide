use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, Paragraph, Wrap},
};

use crate::app::{App, Pane, SETTINGS, View};

const HELP: [&str; 14] = [
    "j / k or ↓ / ↑    Move through rows",
    "gg / G            First or last row",
    "Ctrl+u / Ctrl+d   Move half a page",
    "h / l or Ctrl+h/l Focus list or details",
    "Tab / Shift+Tab   Next or previous view",
    "Alt+Tab / Alt+Shift+Tab   Cycle views",
    "/                 Filter or search",
    "Space e           Open Installed",
    "Space f f         Open package search",
    "Space f t         Go to Theme",
    "Space q or Ctrl+c Quit",
    "",
    "Enter submits a prompt. Esc cancels.",
    "j / k scroll here. Esc closes help.",
];

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    if area.height < 7 || area.width < 12 {
        app.update_layout(0, false);
        frame.render_widget(Paragraph::new("ArchOxide\nResize terminal"), area);
        if app.show_help {
            draw_help(frame, app, area);
        }
        return;
    }

    let prompt = app.prompt();
    let [header, input, content, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(if prompt.is_some() { 3 } else { 0 }),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, app.view, header);
    if let Some((title, text)) = prompt {
        draw_prompt(frame, input, title, text, app.editing());
    }
    draw_content(frame, app, content);
    draw_footer(frame, app, footer);
    if app.show_help {
        draw_help(frame, app, content);
    }
}

fn draw_header(frame: &mut Frame<'_>, active: View, area: Rect) {
    let mut titles = Vec::new();
    if area.width >= 41 {
        titles.push(" ArchOxide  ".bold());
    }
    for view in View::ALL {
        let label = Span::raw(format!(" {} ", view.title()));
        titles.push(if view == active {
            label.fg(Color::Cyan).add_modifier(Modifier::REVERSED)
        } else {
            label
        });
    }
    if area.width < 29 {
        titles = vec![active.title().cyan().bold()];
    }
    frame.render_widget(Line::from(titles), area);
}

fn draw_prompt(frame: &mut Frame<'_>, area: Rect, title: &str, text: &str, editing: bool) {
    let block = panel(title, editing);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if text.is_empty() && !editing {
        frame.render_widget(Paragraph::new("Press / to type".dim()), inner);
    } else {
        let line = Line::raw(text);
        let width = line.width();
        let scroll = if editing {
            width.saturating_sub(usize::from(inner.width.saturating_sub(1)))
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(line).scroll((0, u16::try_from(scroll).unwrap_or(u16::MAX))),
            inner,
        );
        if editing && !inner.is_empty() {
            let column = width.min(usize::from(inner.width - 1)) as u16;
            frame.set_cursor_position((inner.x + column, inner.y));
        }
    }
}

fn draw_content(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let has_details = area.width >= 72 && area.height >= 4;
    let [list_area, details_area] = if has_details {
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area)
    } else {
        [area, Rect::ZERO]
    };
    let (title, status) = match app.view {
        View::Installed => (
            "Packages",
            Some("Package inventory is coming in the next milestone."),
        ),
        View::Search => (
            "Results",
            Some("Repository and AUR search are coming in the next milestone."),
        ),
        View::Settings => ("Settings preview", None),
    };
    let list = panel(title, has_details && app.focus == Pane::List);
    let inner = list.inner(list_area);
    app.update_layout(usize::from(inner.height), has_details);

    if let Some(message) = status {
        frame.render_widget(
            Paragraph::new(message)
                .block(list)
                .wrap(Wrap { trim: true }),
            list_area,
        );
    } else {
        frame.render_stateful_widget(
            List::new(SETTINGS.map(|(label, _)| label))
                .block(list)
                .highlight_style(Style::new().fg(Color::Cyan).bold())
                .highlight_symbol("› "),
            list_area,
            &mut app.selection,
        );
    }

    if has_details {
        let description = if app.view == View::Settings {
            app.selection.selected().map(|index| SETTINGS[index].1)
        } else {
            None
        };
        frame.render_widget(
            Paragraph::new(description.unwrap_or("Package details appear here."))
                .wrap(Wrap { trim: true })
                .block(panel("Details", app.focus == Pane::Details)),
            details_area,
        );
    }
}

fn draw_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let hints = if app.show_help {
        " Esc Close help"
    } else if app.editing() {
        " Enter Apply  Esc Cancel"
    } else if let Some(prefix) = app.bindings.pending_label() {
        frame.render_widget(Line::from(format!(" {prefix} …   Esc Cancel")).cyan(), area);
        return;
    } else if area.width < 44 {
        " ? Help  Tab Views  Space q Quit"
    } else {
        match app.view {
            View::Installed => " ? Help  / Filter  Tab Views  Space q Quit",
            View::Search => " ? Help  / Search  Tab Views  Space q Quit",
            View::Settings => " ? Help  j/k Move  Tab Views  Space q Quit",
        }
    };
    frame.render_widget(Line::raw(hints), area);
}

fn draw_help(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let width = area.width.min(53);
    let height = area.height.min(HELP.len() as u16 + 2);
    let overlay = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let block = panel("Keys", true);
    let visible = block.inner(overlay).height;
    app.help_scroll = app
        .help_scroll
        .min((HELP.len() as u16).saturating_sub(visible));
    frame.render_widget(Clear, overlay);
    frame.render_widget(
        Paragraph::new(HELP.map(Line::raw).to_vec())
            .scroll((app.help_scroll, 0))
            .block(block),
        overlay,
    );
}

fn panel(title: &str, focused: bool) -> Block<'_> {
    let border = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    Block::new()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Line::from(format!(" {title} ")).bold())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use ratatui::{
        Terminal,
        backend::TestBackend,
        crossterm::event::{KeyCode, KeyEvent},
    };

    use super::*;

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn details_pane_appears_only_when_it_fits() {
        let mut app = App::default();
        let wide = render(&mut app, 100, 24);
        assert!(wide.contains("ArchOxide"));
        assert!(wide.contains("Filter installed"));
        assert!(wide.contains("Package details appear here"));

        let narrow = render(&mut app, 40, 14);
        assert!(narrow.contains("Packages"));
        assert!(narrow.contains("? Help"));
        assert!(!narrow.contains("Details"));

        let tiny = render(&mut app, 12, 2);
        assert!(tiny.contains("ArchOxide"));
    }

    #[test]
    fn help_overlays_content_and_closes_with_escape() {
        let mut app = App::default();
        let now = Instant::now();

        app.handle_key(KeyEvent::from(KeyCode::Char('?')), now);
        let help = render(&mut app, 90, 25);
        assert!(help.contains("Move through rows"));
        assert!(help.contains("Esc Close help"));

        app.handle_key(KeyEvent::from(KeyCode::Esc), now);
        let restored = render(&mut app, 90, 25);
        assert!(restored.contains("Package inventory"));
        assert!(!restored.contains("Esc Close help"));
    }
}
