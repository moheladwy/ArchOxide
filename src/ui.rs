use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::{
    app::{App, Pane, ReadState, Setting, View},
    config::{Density, Theme},
    packages::Transaction,
};

const HELP: &[&str] = &[
    "j / k or ↓ / ↑    Move through rows",
    "gg / G            First or last row",
    "Ctrl+u / Ctrl+d   Move half a page",
    "h / l or Ctrl+h/l Focus list or details",
    "Tab / Shift+Tab   Next or previous view",
    "Alt+Tab / Alt+Shift+Tab   Cycle views",
    "/                 Filter or search",
    "a                 Filter by availability",
    "R                 Refresh Installed",
    "i / d             Install / remove selection",
    "Space u           Update all packages",
    "u / U             Update selected / all except selected",
    "x / X             Toggle package mark / clear marks",
    "Space e           Open Installed",
    "Space f f         Open package search",
    "Space f t         Go to Theme",
    "Enter             Edit selected setting",
    "s / r             Save settings / reset draft",
    "Ctrl+u            Clear text in a prompt",
    "Space q / Ctrl+c  Quit",
    "",
    "j / k scroll here. Esc closes help.",
];

#[derive(Clone, Copy)]
struct Palette {
    base: Style,
    muted: Color,
    accent: Color,
    selection: Color,
}

impl Palette {
    fn new(theme: Theme) -> Self {
        let (fg, bg, muted, accent, selection) = match theme {
            Theme::Dark => (
                Color::Rgb(230, 237, 243),
                Color::Rgb(18, 24, 32),
                Color::Rgb(142, 159, 176),
                Color::Rgb(114, 222, 213),
                Color::Rgb(37, 59, 68),
            ),
            Theme::Light => (
                Color::Rgb(25, 38, 50),
                Color::Rgb(246, 249, 252),
                Color::Rgb(80, 98, 116),
                Color::Rgb(0, 102, 111),
                Color::Rgb(210, 233, 236),
            ),
            Theme::Terminal => (
                Color::Reset,
                Color::Reset,
                Color::Reset,
                Color::Cyan,
                Color::Reset,
            ),
        };
        Self {
            base: Style::new().fg(fg).bg(bg),
            muted,
            accent,
            selection,
        }
    }

    fn highlight(self) -> Style {
        if self.selection == Color::Reset {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new().fg(self.accent).bg(self.selection).bold()
        }
    }

    fn panel(self, title: &str, focused: bool) -> Block<'_> {
        let border = if focused { self.accent } else { self.muted };
        Block::new()
            .borders(Borders::ALL)
            .border_style(border)
            .title(Line::from(format!(" {title} ")).bold())
    }
}

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let palette = Palette::new(app.config.appearance.theme);
    frame.render_widget(Block::new().style(palette.base), area);

    if area.height < 7 || area.width < 12 {
        app.update_layout(0, false);
        frame.render_widget(Paragraph::new("ArchOxide\nResize terminal"), area);
        if app.show_help {
            draw_help(frame, app, area, palette);
        }
        if let Some(transaction) = &app.confirmation {
            draw_confirmation(
                frame,
                transaction,
                &mut app.confirmation_scroll,
                area,
                palette,
            );
        }
        return;
    }

    let prompt = app.prompt();
    let prompt_height = if prompt.is_some() { 3 } else { 0 };
    let status = status_lines(app, palette);
    let status_height = (status.len() as u16).min(area.height.saturating_sub(prompt_height + 4));
    let [header, input, content, status_area, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(prompt_height),
        Constraint::Min(2),
        Constraint::Length(status_height),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, app.view, header, palette);
    if let Some((title, text)) = prompt {
        draw_prompt(frame, input, title, text, app.editing(), palette);
    }
    draw_content(frame, app, content, palette);
    frame.render_widget(Paragraph::new(status), status_area);
    draw_footer(frame, app, footer, palette);
    if app.show_help {
        draw_help(frame, app, content, palette);
    }
    if let Some(transaction) = &app.confirmation {
        draw_confirmation(
            frame,
            transaction,
            &mut app.confirmation_scroll,
            area,
            palette,
        );
    }
}

fn draw_header(frame: &mut Frame<'_>, active: View, area: Rect, palette: Palette) {
    if area.width < 29 {
        frame.render_widget(Line::from(active.title()).style(palette.highlight()), area);
        return;
    }
    let mut labels = Vec::new();
    if area.width >= 41 {
        labels.push(" ArchOxide  ".bold());
    }
    for view in View::ALL {
        let label = Span::raw(format!(" {} ", view.title()));
        labels.push(if view == active {
            label.style(palette.highlight())
        } else {
            label
        });
    }
    frame.render_widget(Line::from(labels), area);
}

fn draw_prompt(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    text: &str,
    editing: bool,
    palette: Palette,
) {
    let block = palette.panel(title, editing);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if text.is_empty() && !editing {
        frame.render_widget(Paragraph::new("Press / to type").fg(palette.muted), inner);
        return;
    }

    let line = Line::raw(text);
    let width = line.width();
    let max_column = inner.width.saturating_sub(1);
    let scroll = if editing {
        width.saturating_sub(usize::from(max_column))
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(line).scroll((0, u16::try_from(scroll).unwrap_or(u16::MAX))),
        inner,
    );
    if editing && !inner.is_empty() {
        let column = width.min(usize::from(max_column)) as u16;
        frame.set_cursor_position((inner.x + column, inner.y));
    }
}

fn draw_content(frame: &mut Frame<'_>, app: &mut App, area: Rect, palette: Palette) {
    let has_details = area.width >= 72 && area.height >= 4;
    let [list_area, details_area] = if has_details {
        let width = u16::from(app.config.appearance.details_width_percent);
        Layout::horizontal([
            Constraint::Percentage(100 - width),
            Constraint::Percentage(width),
        ])
        .areas(area)
    } else {
        [area, Rect::ZERO]
    };
    let title = match app.view {
        View::Installed => "Packages",
        View::Search => "Results",
        View::Settings => "Preferences",
    };
    let base = palette.panel(title, has_details && app.focus == Pane::List);
    let comfortable = app.config.appearance.density == Density::Comfortable;
    let block = if comfortable && list_area.width >= 28 {
        base.padding(ratatui::widgets::Padding::horizontal(1))
    } else {
        base
    };
    let inner = block.inner(list_area);
    let row_height = if comfortable && app.view != View::Settings {
        2
    } else {
        1
    };
    app.update_layout(usize::from(inner.height / row_height), has_details);

    let rows: Vec<ListItem<'static>> = match app.view {
        View::Installed => app
            .installed_rows()
            .map(|package| {
                package_row(
                    &package.name,
                    &package.version,
                    package.availability.label(),
                    app.is_marked(&package.name),
                    comfortable,
                    palette,
                )
            })
            .collect(),
        View::Search => app
            .search_rows()
            .map(|package| {
                package_row(
                    &package.name,
                    &package.version,
                    &package.source.to_string(),
                    false,
                    comfortable,
                    palette,
                )
            })
            .collect(),
        View::Settings => Setting::ALL
            .into_iter()
            .map(|setting| {
                let value = app.setting_value(setting);
                let line = if value.is_empty() {
                    Line::from(setting.label()).fg(palette.accent)
                } else {
                    Line::from(vec![
                        Span::raw(format!("{:<16}", setting.label())),
                        value.fg(palette.muted),
                    ])
                };
                ListItem::new(line)
            })
            .collect(),
    };
    if rows.is_empty() {
        let message = match app.view {
            View::Installed => match app.installed_state {
                ReadState::Idle | ReadState::Loading => "Reading installed packages…",
                ReadState::Ready => "No installed packages match.",
                ReadState::Failed(_) => "Installed packages could not be read.",
            },
            View::Search if app.query.is_empty() => {
                "Press / to search by package name or description."
            }
            View::Search if app.search_states.contains(&ReadState::Loading) => "Searching…",
            View::Search => "No results from the available sources.",
            View::Settings => "",
        };
        frame.render_widget(
            Paragraph::new(message)
                .block(block)
                .wrap(Wrap { trim: true }),
            list_area,
        );
    } else {
        frame.render_stateful_widget(
            List::new(rows)
                .block(block)
                .highlight_style(palette.highlight())
                .highlight_symbol("› "),
            list_area,
            app.selection_mut(),
        );
    }

    if has_details {
        draw_details(frame, app, details_area, palette);
    }
}

fn package_row(
    name: &str,
    version: &str,
    source: &str,
    marked: bool,
    comfortable: bool,
    palette: Palette,
) -> ListItem<'static> {
    let name = if marked {
        format!("[x] {name}")
    } else {
        name.to_owned()
    };
    if comfortable {
        ListItem::new(vec![
            Line::raw(name),
            Line::raw(format!("{source}  ·  {version}")).fg(palette.muted),
        ])
    } else {
        ListItem::new(Line::from(vec![
            Span::raw(format!("{name}  ")),
            format!("{source}  ·  {version}").fg(palette.muted),
        ]))
    }
}

fn draw_details(frame: &mut Frame<'_>, app: &App, area: Rect, palette: Palette) {
    let lines = match app.view {
        View::Installed => app.selected_installed().map(|package| {
            vec![
                Line::from(package.name.as_str()).bold(),
                Line::raw(format!("Installed version {}", package.version)),
                Line::raw(format!("Availability {}", package.availability.label())),
            ]
        }),
        View::Search => app.selected_result().map(|package| {
            let installed = app.installed_version(&package.name).unwrap_or("—");
            vec![
                Line::from(package.name.as_str()).bold(),
                Line::raw(format!("Source {}", package.source)),
                Line::raw(format!("Available {}", package.version)),
                Line::raw(format!("Installed {installed}")),
                Line::raw(""),
                Line::raw(package.description.as_str()),
            ]
        }),
        View::Settings => app.selected_setting().map(|setting| {
            vec![
                Line::from(setting.label()).bold(),
                Line::raw(setting.description()),
            ]
        }),
    };
    let block = palette.panel("Details", app.focus == Pane::Details);
    if let Some(lines) = lines {
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
            area,
        );
    } else {
        frame.render_widget(block, area);
    }
}

fn one_line(message: &str) -> String {
    message
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn status_lines(app: &App, palette: Palette) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(notice) = &app.notice {
        lines.push(Line::raw(one_line(notice)).fg(palette.accent));
    }

    match app.view {
        View::Installed => {
            if let ReadState::Failed(error) = &app.installed_state {
                lines.push(Line::raw(one_line(error)).fg(palette.accent));
            }
            if let Some(warning) = &app.installed_warning {
                lines.push(Line::raw(one_line(warning)).fg(palette.accent));
            }
            let matching = app.installed_rows().count();
            let marked = app.marked_count();
            let marks = if marked == 0 {
                String::new()
            } else {
                format!("  ·  {marked} marked")
            };
            lines.push(
                Line::raw(format!(
                    " {matching} packages{marks}  ·  Availability {}",
                    app.availability_label()
                ))
                .fg(palette.muted),
            );
        }
        View::Search => {
            if !app.config.packages.aur_enabled {
                lines.push(Line::raw(" AUR disabled in Settings").fg(palette.muted));
            } else if !app.yay_available {
                lines.push(Line::raw(" AUR unavailable  ·  yay was not found").fg(palette.muted));
            }
            for (label, state) in ["Repositories", "AUR"].into_iter().zip(&app.search_states) {
                match state {
                    ReadState::Loading => {
                        lines.push(Line::raw(format!(" {label}  ·  Searching…")).fg(palette.muted))
                    }
                    ReadState::Failed(error) => lines.push(
                        Line::raw(format!(" {label}  ·  {}", one_line(error))).fg(palette.accent),
                    ),
                    ReadState::Idle | ReadState::Ready => {}
                }
            }
        }
        View::Settings if app.settings_dirty() => {
            lines.push(Line::raw(" Unsaved changes").fg(palette.accent));
        }
        View::Settings => {}
    }
    lines
}

fn draw_footer(frame: &mut Frame<'_>, app: &App, area: Rect, palette: Palette) {
    let hints = if app.confirmation.is_some() {
        " Enter Confirm  Esc Cancel"
    } else if app.show_help {
        " Esc Close help"
    } else if app.editing() {
        " Enter Apply  Esc Cancel  Ctrl+u Clear"
    } else if let Some(prefix) = app.bindings.pending_label() {
        frame.render_widget(
            Line::from(format!(" {prefix} …   Esc Cancel")).fg(palette.accent),
            area,
        );
        return;
    } else {
        match app.view {
            View::Installed => {
                " ? Help  x Mark  u Selected  U Except  Space u All  / Filter  d Remove  Tab Views"
            }
            View::Search => " ? Help  / Search  i Install  Space u All  Tab Views",
            View::Settings => " ? Help  Enter Edit  s Save  r Reset  Tab Views",
        }
    };
    frame.render_widget(Line::raw(hints), area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn draw_help(frame: &mut Frame<'_>, app: &mut App, area: Rect, palette: Palette) {
    let overlay = centered(area, 53, HELP.len() as u16 + 2);
    let block = palette.panel("Keys", true).style(palette.base);
    let visible = block.inner(overlay).height;
    app.help_scroll = app
        .help_scroll
        .min((HELP.len() as u16).saturating_sub(visible));
    frame.render_widget(Clear, overlay);
    frame.render_widget(
        Paragraph::new(HELP.iter().copied().map(Line::raw).collect::<Vec<_>>())
            .scroll((app.help_scroll, 0))
            .block(block),
        overlay,
    );
}

fn draw_confirmation(
    frame: &mut Frame<'_>,
    transaction: &Transaction,
    scroll: &mut u16,
    area: Rect,
    palette: Palette,
) {
    let (title, target, source, names) = match transaction {
        Transaction::Install(package) => (
            "Install package",
            format!("{} {}", package.name, package.version),
            format!("Source {}", package.source),
            None,
        ),
        Transaction::Remove(package) => (
            "Remove package",
            format!("{} {}", package.name, package.version),
            format!("Installed package · {}", package.availability.label()),
            None,
        ),
        Transaction::Upgrade { aur } => (
            "Full system upgrade",
            "Refresh databases and upgrade installed packages".into(),
            upgrade_sources(*aur).into(),
            None,
        ),
        Transaction::UpgradeSelected { aur, names } => (
            "Update selected packages",
            format!("{} selected · Use cached repository databases", names.len()),
            upgrade_sources(*aur).into(),
            Some(names),
        ),
        Transaction::UpgradeExcept { aur, names } => (
            "Update all except selected",
            format!(
                "{} excluded · Refresh databases and update the rest",
                names.len()
            ),
            upgrade_sources(*aur).into(),
            Some(names),
        ),
    };
    let height = names.map_or(8, |names| (names.len().saturating_add(8)).min(18) as u16);
    let area = centered(area, 70, height);
    let mut lines = vec![Line::from(target).bold(), Line::raw(source)];
    if let Some(names) = names {
        lines.push(Line::raw("Partial upgrades can break dependencies.").fg(palette.accent));
        lines.push(Line::raw(""));
        lines.extend(names.iter().map(|name| Line::raw(name.as_str())));
    }

    let block = palette.panel(title, true).style(palette.base);
    let [body, controls] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(block.inner(area));
    let width = usize::from(body.width.max(1));
    let content_height = lines
        .iter()
        .map(|line| line.width().div_ceil(width).max(1))
        .sum::<usize>();
    let max_scroll =
        u16::try_from(content_height.saturating_sub(usize::from(body.height))).unwrap_or(u16::MAX);
    *scroll = (*scroll).min(max_scroll);
    let hints = if max_scroll > 0 {
        "Enter Confirm   Esc Cancel   j/k Scroll"
    } else {
        "Enter Confirm   Esc Cancel"
    };

    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((*scroll, 0)),
        body,
    );
    frame.render_widget(Line::raw(hints).fg(palette.accent), controls);
}

fn upgrade_sources(aur: bool) -> &'static str {
    if aur {
        "Configured repositories and AUR (yay)"
    } else {
        "Configured repositories (pacman)"
    }
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
        assert!(wide.contains("Details"));

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
        assert!(restored.contains("Reading installed packages"));
        assert!(!restored.contains("Esc Close help"));
    }

    #[test]
    fn saved_theme_and_layout_control_the_rendered_view() {
        let mut app = App::default();
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let dark_background = terminal.backend().buffer()[(99, 0)].bg;
        assert_eq!(terminal.backend().buffer()[(60, 4)].symbol(), "┌");

        app.config.appearance.theme = Theme::Light;
        app.config.appearance.details_width_percent = 50;
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        assert_ne!(buffer[(99, 0)].bg, dark_background);
        assert_eq!(buffer[(50, 4)].symbol(), "┌");

        let palette = Palette::new(Theme::Light);
        assert_eq!(
            package_row("test", "1", "extra", false, true, palette).height(),
            2
        );
        assert_eq!(
            package_row("test", "1", "extra", false, false, palette).height(),
            1
        );
    }
}
