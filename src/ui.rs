use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, HighlightSpacing, List, ListItem, Paragraph, Wrap},
};
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::{
    app::{App, PackageInfo, Pane, ReadState, Setting, View},
    appimage,
    config::{Density, Theme},
    flatpak,
    packages::{SearchResult, Source, Transaction},
    picker::{Kind as PickerKind, Picker},
    source_views::flatpak_key,
    transaction_terminal::{self, TransactionTerminal},
};

const HELP: &[&str] = &[
    "j / k or ↓ / ↑    Move through rows",
    "gg / G or Ctrl+Home / Ctrl+End   First / last row",
    "Ctrl+u / Ctrl+d   Move half a page",
    "h / l or Ctrl+h/l Focus list or details",
    "Tab / Shift+Tab   Next or previous view",
    "Alt+Tab / Alt+Shift+Tab   Cycle views",
    "/                 Filter or search",
    "a                 Filter by availability",
    "R                 Refresh the current list",
    "i / d             Install / remove selection",
    "Space u           Update all packages",
    "u / U             Update selected / all except selected",
    "x / X             Toggle package mark / clear marks",
    "Space e           Open Installed",
    "Space p / Space a Open Flatpak / AppImages",
    "Space o           Open desktop Apps",
    "f                 Search Flatpak remotes",
    "Esc               Return to installed Flatpaks",
    "i / e             Import / replace an AppImage",
    "Space f f         Open package search",
    "Space f t         Go to Theme",
    "Enter             Open selected app or edit setting",
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
        let (fg, bg, muted, accent, selection) = theme.colors();
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
    let palette = Palette::new(app.rendered_theme());
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
        if let Some(picker) = &mut app.picker {
            draw_picker(frame, picker, area, palette);
        }
        return;
    }

    let prompt = app.prompt();
    let prompt_height = if prompt.is_some() { 3 } else { 0 };
    let status = status_lines(app, palette);
    let wrap_status = app.view == View::Apps;
    let status_rows = if wrap_status {
        let width = usize::from(area.width);
        status
            .iter()
            .map(|line| line.width().div_ceil(width).max(1))
            .sum()
    } else {
        status.len()
    };
    let status_height = (status_rows as u16).min(area.height.saturating_sub(prompt_height + 4));
    let [header, input, content, status_area, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(prompt_height),
        Constraint::Min(2),
        Constraint::Length(status_height),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, app, header, palette);
    if let Some((title, text)) = prompt {
        draw_prompt(frame, input, title, text, app.editing(), palette);
    }
    draw_content(frame, app, content, palette);
    let status = Paragraph::new(status);
    let status = if wrap_status {
        status.wrap(Wrap { trim: true })
    } else {
        status
    };
    frame.render_widget(status, status_area);
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
    if let Some(picker) = &mut app.picker {
        draw_picker(frame, picker, area, palette);
    }
}

pub fn draw_package_terminal(frame: &mut Frame<'_>, app: &App, terminal: &TransactionTerminal) {
    let palette = Palette::new(app.rendered_theme());
    let (popup, console, footer) = transaction_terminal::areas(frame.area());
    frame.render_widget(Clear, popup);
    frame.render_widget(
        palette.panel("Package terminal", true).style(palette.base),
        popup,
    );
    let cursor = Cursor::default()
        .visibility(terminal.accepts_input() && terminal.screen().scrollback() == 0)
        .style(Style::new().fg(palette.accent));
    frame.render_widget(
        PseudoTerminal::new(terminal.screen())
            .style(palette.base)
            .cursor(cursor),
        console,
    );
    let hints = if !terminal.is_running() {
        " Enter/Esc Close  ·  PgUp/PgDn Scroll"
    } else if terminal.accepts_input() {
        " Ctrl+C Interrupt  ·  Shift+PgUp/PgDn Scroll"
    } else {
        " Running…  ·  Shift+PgUp/PgDn Scroll"
    };
    frame.render_widget(Line::raw(hints).fg(palette.accent), footer);
}

fn draw_header(frame: &mut Frame<'_>, app: &App, area: Rect, palette: Palette) {
    let tabs_width = app
        .views()
        .map(|view| view.title().len() + 2)
        .sum::<usize>();
    if usize::from(area.width) < tabs_width {
        frame.render_widget(
            Line::from(app.view.title()).style(palette.highlight()),
            area,
        );
        return;
    }
    let mut labels = Vec::new();
    if usize::from(area.width) >= tabs_width + " ArchOxide  ".len() {
        labels.push(" ArchOxide  ".bold());
    }
    for view in app.views() {
        let label = Span::raw(format!(" {} ", view.title()));
        labels.push(if view == app.view {
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
    if !has_details && app.focus == Pane::Details && app.view.has_package_details() {
        app.update_layout(0, true);
        draw_details(frame, app, area, palette);
        return;
    }
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
        View::Flatpak if app.sources.flatpak_query.is_some() => "Flatpak results",
        View::Flatpak => "Installed Flatpaks",
        View::AppImages => "Managed AppImages",
        View::Apps => "Desktop apps",
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
            .map(|package| search_row(package, inner.width.saturating_sub(2), comfortable, palette))
            .collect(),
        View::Flatpak if app.sources.flatpak_query.is_some() => app
            .sources
            .flatpak_results
            .iter()
            .map(|entry| {
                package_row(
                    app_name(&entry.name, &entry.id),
                    &entry.version,
                    &installation_label(&entry.remote, &entry.installation),
                    false,
                    comfortable,
                    palette,
                )
            })
            .collect(),
        View::Flatpak => app
            .flatpak_rows()
            .map(|entry| {
                package_row(
                    app_name(&entry.name, &entry.id),
                    &entry.version,
                    &installation_label(&entry.remote, &entry.installation),
                    app.sources.flatpaks.marks.contains(&flatpak_key(entry)),
                    comfortable,
                    palette,
                )
            })
            .collect(),
        View::AppImages => app
            .image_rows()
            .map(|entry| {
                let source = match entry.origin.as_deref() {
                    Some(origin) if origin.starts_with("https://") => "HTTPS download",
                    Some(_) => "Local file",
                    None => "Managed AppImage",
                };
                package_row(
                    &entry.name,
                    "",
                    source,
                    app.sources.images.marks.contains(&entry.path),
                    comfortable,
                    palette,
                )
            })
            .collect(),
        View::Apps => app
            .desktop_app_rows()
            .map(|entry| {
                package_row(
                    &one_line(&entry.name),
                    "",
                    &one_line(&entry.comment),
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
                        Span::raw(format!("{:<18}", setting.label())),
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
            View::Flatpak => {
                let (state, ready) = if app.sources.flatpak_query.is_some() {
                    (
                        &app.sources.flatpak_search,
                        "No matching apps in configured Flatpak remotes.",
                    )
                } else {
                    (
                        &app.sources.flatpaks.state,
                        "No installed Flatpaks match. Press f to search remotes.",
                    )
                };
                match state {
                    ReadState::Idle | ReadState::Loading => "Reading Flatpak apps…",
                    ReadState::Ready => ready,
                    ReadState::Failed(_) => "Flatpak apps could not be read.",
                }
            }
            View::AppImages => match app.sources.images.state {
                ReadState::Idle | ReadState::Loading => "Reading managed AppImages…",
                ReadState::Ready if app.sources.images.entries.is_empty() => {
                    "Press i to import an AppImage from a local file or direct HTTPS URL."
                }
                ReadState::Ready => "No managed AppImages match.",
                ReadState::Failed(_) => "Managed AppImages could not be read.",
            },
            View::Apps => match app.desktop_apps.state {
                ReadState::Idle | ReadState::Loading => "Reading desktop apps…",
                ReadState::Ready => "No desktop apps match.",
                ReadState::Failed(_) => "Desktop apps could not be read.",
            },
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
                .highlight_symbol("› ")
                .highlight_spacing(if app.view == View::Search {
                    HighlightSpacing::Always
                } else {
                    HighlightSpacing::WhenSelected
                }),
            list_area,
            app.selection_mut(),
        );
    }

    if has_details {
        draw_details(frame, app, details_area, palette);
    }
}

fn app_name<'a>(name: &'a str, id: &'a str) -> &'a str {
    if name.is_empty() { id } else { name }
}

fn installation_label(remote: &str, installation: &flatpak::Installation) -> String {
    if remote.is_empty() {
        installation.to_string()
    } else {
        format!("{remote} · {installation}")
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
    let metadata = if version.is_empty() {
        source.to_owned()
    } else {
        format!("{source}  ·  {version}")
    };
    if comfortable {
        ListItem::new(vec![Line::raw(name), Line::raw(metadata).fg(palette.muted)])
    } else {
        ListItem::new(Line::from(vec![
            Span::raw(format!("{name}  ")),
            metadata.fg(palette.muted),
        ]))
    }
}

// Reserve the right edge for the source type, even when a name is too long.
fn search_row(
    package: &SearchResult,
    width: u16,
    comfortable: bool,
    palette: Palette,
) -> ListItem<'static> {
    let width = usize::from(width);
    let source = package.source.label();
    let left_width = width.saturating_sub(source.len() + 1);
    let name = fit_search_text(&package.name, left_width);
    let mut left = vec![Span::raw(name.clone())];
    let mut used = Line::raw(name).width();
    if !comfortable && !package.version.is_empty() {
        let version = format!("  ·  {}", package.version);
        let version = fit_search_text(&version, left_width.saturating_sub(used));
        used += Line::raw(version.as_str()).width();
        left.push(version.fg(palette.muted));
    }
    let gap = width.saturating_sub(used + source.len());
    left.push(Span::raw(" ".repeat(gap)));
    left.push(source.fg(palette.accent));
    let first = Line::from(left);
    if comfortable {
        let metadata = if package.version.is_empty() {
            package.source.to_string()
        } else {
            format!("{}  ·  {}", package.source, package.version)
        };
        ListItem::new(vec![first, Line::raw(metadata).fg(palette.muted)])
    } else {
        ListItem::new(first)
    }
}

fn fit_search_text(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if Line::raw(text).width() <= width {
        return text.to_owned();
    }
    let mut result = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = Span::raw(character.to_string()).width();
        if used + character_width >= width {
            break;
        }
        result.push(character);
        used += character_width;
    }
    result.push('…');
    result
}

fn draw_details(frame: &mut Frame<'_>, app: &mut App, area: Rect, palette: Palette) {
    let lines = match app.view {
        View::Installed => app.selected_installed().map(|package| {
            vec![
                Line::from(package.name.as_str()).bold(),
                Line::raw(format!("Installed version {}", package.version)),
                Line::raw(format!("Availability {}", package.availability.label())),
            ]
        }),
        View::Search => app.selected_result().map(|package| {
            let mut lines = vec![
                Line::from(package.name.as_str()).bold(),
                Line::raw(format!("Source {}", package.source)),
            ];
            if !package.version.is_empty() {
                lines.push(Line::raw(format!("Available {}", package.version)));
            }
            match &package.source {
                Source::Repository(_) | Source::Aur => {
                    let installed = app.installed_version(&package.name).unwrap_or("—");
                    lines.push(Line::raw(format!("Installed {installed}")));
                }
                Source::Flatpak { reference, .. } => {
                    lines.push(Line::raw(reference.as_str()));
                }
                Source::ManagedAppImage(path) => {
                    lines.push(Line::raw(one_line(&path.display().to_string())));
                }
                Source::AppImageImport(_) => {}
            }
            lines.push(Line::raw(""));
            lines.push(Line::raw(one_line(&package.description)));
            lines
        }),
        View::Flatpak if app.sources.flatpak_query.is_some() => {
            app.selected_flatpak_result().map(|entry| {
                vec![
                    Line::from(app_name(&entry.name, &entry.id)).bold(),
                    Line::raw(entry.reference.as_str()),
                    Line::raw(installation_label(&entry.remote, &entry.installation)),
                    Line::raw(entry.description.as_str()),
                ]
            })
        }
        View::Flatpak => app.selected_flatpak().map(|entry| {
            vec![
                Line::from(app_name(&entry.name, &entry.id)).bold(),
                Line::raw(entry.reference.as_str()),
                Line::raw(installation_label(&entry.remote, &entry.installation)),
                Line::raw(format!("Installed version {}", entry.version)),
            ]
        }),
        View::AppImages => app.selected_image().map(|entry| {
            vec![
                Line::from(entry.name.as_str()).bold(),
                Line::raw(entry.path.display().to_string()),
                Line::raw(""),
                Line::raw("Saved source"),
                Line::raw(entry.origin.as_deref().unwrap_or("—")),
            ]
        }),
        View::Apps => app.selected_desktop_app().map(|entry| {
            vec![
                Line::from(one_line(&entry.name)).bold(),
                Line::raw(one_line(&entry.id)),
                Line::raw(one_line(&entry.comment)),
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
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if let Some(mut lines) = lines {
        match &app.package_details.info {
            Some(PackageInfo::Loading) => {
                lines.push(Line::raw(""));
                lines.push(Line::raw("Loading package information…").fg(palette.muted));
            }
            Some(PackageInfo::Ready(info)) if !info.is_empty() => {
                lines.push(Line::raw(""));
                lines.extend(info.lines().map(|line| Line::raw(one_line(line))));
            }
            Some(PackageInfo::Failed(error)) => {
                lines.push(Line::raw(""));
                lines.push(Line::raw(one_line(error)).fg(palette.accent));
            }
            Some(PackageInfo::Ready(_)) | None => {}
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        let hidden_rows = paragraph
            .line_count(inner.width)
            .saturating_sub(usize::from(inner.height));
        let scroll_limit = u16::try_from(hidden_rows).unwrap_or(u16::MAX);
        let scroll = app.details_scroll().min(scroll_limit);
        frame.render_widget(paragraph.scroll((scroll, 0)), inner);
        app.update_details_layout(scroll_limit, inner.height);
    } else {
        if app.focus == Pane::Details && app.view.has_package_details() {
            frame.render_widget(
                Paragraph::new("Select a package to view its details."),
                inner,
            );
        }
        app.update_details_layout(0, inner.height);
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
            for (label, state) in ["Pacman", "AUR", "Flatpak", "AppImage"]
                .into_iter()
                .zip(&app.search_states)
            {
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
        View::Flatpak => {
            let (state, count, marks) = if app.sources.flatpak_query.is_some() {
                (
                    &app.sources.flatpak_search,
                    app.sources.flatpak_results.len(),
                    0,
                )
            } else {
                (
                    &app.sources.flatpaks.state,
                    app.flatpak_rows().count(),
                    app.sources.flatpaks.marks.len(),
                )
            };
            app_source_status(&mut lines, state, count, marks, palette);
        }
        View::AppImages => app_source_status(
            &mut lines,
            &app.sources.images.state,
            app.image_rows().count(),
            app.sources.images.marks.len(),
            palette,
        ),
        View::Apps => app_source_status(
            &mut lines,
            &app.desktop_apps.state,
            app.desktop_app_rows().count(),
            0,
            palette,
        ),
        View::Settings if app.settings_dirty() => {
            lines.push(Line::raw(" Unsaved changes").fg(palette.accent));
        }
        View::Settings => {}
    }
    lines
}

fn app_source_status(
    lines: &mut Vec<Line<'static>>,
    state: &ReadState,
    count: usize,
    marks: usize,
    palette: Palette,
) {
    match state {
        ReadState::Failed(error) => lines.push(Line::raw(one_line(error)).fg(palette.accent)),
        ReadState::Ready if marks == 0 => {
            lines.push(Line::raw(format!(" {count} apps")).fg(palette.muted))
        }
        ReadState::Ready => {
            lines.push(Line::raw(format!(" {count} apps  ·  {marks} marked")).fg(palette.muted))
        }
        _ => {}
    }
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
    } else if app.focus == Pane::Details && app.view.has_package_details() {
        " h Back  j/k Scroll  Ctrl+u/d Page  gg/G First/last"
    } else {
        match app.view {
            View::Installed => {
                " ? Help  l Details  / Filter  x Mark  u Selected  U Except  Space u All  d Remove"
            }
            View::Search => " ? Help  l Details  / Search  i Install  Space u All  Tab Views",
            View::Flatpak if app.sources.flatpak_query.is_some() => {
                " ? Help  l Details  f Search  i Install  Esc Installed  Tab Views"
            }
            View::Flatpak => {
                " ? Help  l Details  f Search  / Filter  d Remove  u/U Update  Space u All  x Mark"
            }
            View::AppImages => {
                " ? Help  l Details  i Import  e Replace  d Remove  u/U Update  Space u All  x Mark"
            }
            View::Apps => " Enter Open  / Filter  R Refresh  Tab Views  ? Help",
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

fn draw_picker(frame: &mut Frame<'_>, picker: &mut Picker, area: Rect, palette: Palette) {
    let title = match picker.kind {
        PickerKind::Theme => "Choose a theme",
        PickerKind::Font => "Choose an installed font",
    };
    let area = centered(area, 70, 24);
    let block = palette.panel(title, true).style(palette.base);
    let [filter, list, status, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(block.inner(area));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    draw_prompt(
        frame,
        filter,
        "Filter choices",
        &picker.query,
        picker.editing,
        palette,
    );
    let items: Vec<_> = picker
        .visible()
        .map(|choice| ListItem::new(choice.label().to_owned()))
        .collect();
    let status_text = if picker.loading {
        "Reading installed fonts…"
    } else if let Some(error) = &picker.error {
        error
    } else if items.is_empty() {
        "No matching choices."
    } else if picker.kind == PickerKind::Theme {
        "Theme preview follows the highlighted choice."
    } else {
        "Saved fonts apply on the next --kitty launch."
    };
    frame.render_widget(Line::raw(one_line(status_text)).fg(palette.muted), status);
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(palette.highlight())
            .highlight_symbol("› "),
        list,
        &mut picker.selection,
    );
    frame.render_widget(
        Line::raw("Enter Choose  Esc Back  / Filter  ↑↓ Move").fg(palette.accent),
        footer,
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
            Vec::new(),
        ),
        Transaction::Remove(package) => (
            "Remove package",
            format!("{} {}", package.name, package.version),
            format!("Installed package · {}", package.availability.label()),
            Vec::new(),
        ),
        Transaction::Upgrade { aur } => (
            "Full system upgrade",
            "Refresh databases and upgrade installed packages".into(),
            upgrade_sources(*aur).into(),
            Vec::new(),
        ),
        Transaction::UpgradeSelected { aur, names } => (
            "Update selected packages",
            format!("{} selected · Use cached repository databases", names.len()),
            upgrade_sources(*aur).into(),
            names.clone(),
        ),
        Transaction::UpgradeExcept { aur, names } => (
            "Update all except selected",
            format!(
                "{} excluded · Refresh databases and update the rest",
                names.len()
            ),
            upgrade_sources(*aur).into(),
            names.clone(),
        ),
        Transaction::Flatpak(operation) => flatpak_confirmation(operation),
        Transaction::AppImage(operation) => image_confirmation(operation),
    };
    let height = (names.len().saturating_add(8)).min(18) as u16;
    let area = centered(area, 70, height);
    let mut lines = vec![
        Line::from(one_line(&target)).bold(),
        Line::raw(one_line(&source)),
    ];
    if matches!(
        transaction,
        Transaction::UpgradeSelected { .. } | Transaction::UpgradeExcept { .. }
    ) {
        lines.push(Line::raw("Partial upgrades can break dependencies.").fg(palette.accent));
    }
    if !names.is_empty() {
        lines.push(Line::raw(""));
        lines.extend(names.into_iter().map(|name| Line::raw(one_line(&name))));
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

fn flatpak_confirmation(
    operation: &flatpak::Operation,
) -> (&'static str, String, String, Vec<String>) {
    let (title, count, apps) = match operation {
        flatpak::Operation::Install(app) => {
            return (
                "Install Flatpak",
                app.reference.clone(),
                installation_label(&app.remote, &app.installation),
                Vec::new(),
            );
        }
        flatpak::Operation::Remove(app) => {
            return (
                "Remove Flatpak",
                app.reference.clone(),
                app.installation.to_string(),
                Vec::new(),
            );
        }
        flatpak::Operation::UpdateAll => {
            return (
                "Update all Flatpaks",
                "Update applications and runtimes".into(),
                "Configured Flatpak installations".into(),
                Vec::new(),
            );
        }
        flatpak::Operation::Update(apps) => ("Update selected Flatpaks", "selected", apps),
        flatpak::Operation::UpdateExcept(apps) => {
            ("Update all except selected Flatpaks", "excluded", apps)
        }
    };
    let names = apps
        .iter()
        .map(|app| format!("{} · {}", app.reference, app.installation))
        .collect();
    (
        title,
        format!("{} {count}", apps.len()),
        "Installed Flatpak applications".into(),
        names,
    )
}

fn image_confirmation(
    operation: &appimage::Operation,
) -> (&'static str, String, String, Vec<String>) {
    match operation {
        appimage::Operation::Import(source) => (
            "Import AppImage",
            "Copy into managed AppImages".into(),
            source.clone(),
            Vec::new(),
        ),
        appimage::Operation::Remove(entry) => (
            "Remove managed AppImage",
            entry.name.clone(),
            entry.path.display().to_string(),
            Vec::new(),
        ),
        appimage::Operation::Replace(entry, source) => (
            "Replace AppImage",
            entry.name.clone(),
            source.clone(),
            Vec::new(),
        ),
        appimage::Operation::Update(entries) => (
            "Update AppImages",
            format!("{} managed apps", entries.len()),
            "Refresh from saved sources".into(),
            entries.iter().map(|entry| entry.name.clone()).collect(),
        ),
    }
}

fn upgrade_sources(aur: bool) -> &'static str {
    if aur {
        "Configured repositories and AUR (yay)"
    } else {
        "Configured repositories (pacman)"
    }
}

#[cfg(test)]
#[path = "../tests/unit/ui.rs"]
mod tests;
