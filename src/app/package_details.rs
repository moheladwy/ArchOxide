use super::{Action, App, Request, View};
use crate::package_details::Target;

#[derive(Debug)]
pub enum Info {
    Loading,
    Ready(String),
    Failed(String),
}

#[derive(Debug, Default)]
pub struct PackageDetails {
    target: Option<Target>,
    pub(super) revision: u64,
    pub info: Option<Info>,
    scroll: u16,
    scroll_limit: u16,
    viewport_rows: u16,
}

impl App {
    pub(super) fn sync_package_details(&mut self, visible: bool) {
        let target = if visible {
            match self.view {
                View::Installed => self.selected_installed().cloned().map(Target::Installed),
                View::Search => self.selected_result().cloned().map(Target::Search),
                View::Flatpak if self.sources.flatpak_query.is_some() => self
                    .selected_flatpak_result()
                    .cloned()
                    .map(Target::FlatpakRemote),
                View::Flatpak => self
                    .selected_flatpak()
                    .cloned()
                    .map(Target::FlatpakInstalled),
                View::AppImages => self
                    .selected_image()
                    .map(|entry| Target::AppImage(entry.path.clone())),
                View::Apps | View::Settings => None,
            }
        } else {
            None
        };
        if self.package_details.target == target {
            return;
        }

        let details = &mut self.package_details;
        details.target = target.clone();
        details.revision = details.revision.wrapping_add(1);
        details.scroll = 0;
        details.scroll_limit = 0;
        details.info = target.as_ref().map(|_| Info::Loading);
        if let Some(target) = target {
            self.reads
                .submit(Request::PackageInfo(details.revision, target));
        } else {
            self.reads.clear_queued_package_info();
        }
    }

    pub fn details_scroll(&self) -> u16 {
        self.package_details.scroll
    }

    pub fn update_details_layout(&mut self, scroll_limit: u16, viewport_rows: u16) {
        let details = &mut self.package_details;
        details.scroll_limit = scroll_limit;
        details.viewport_rows = viewport_rows;
        details.scroll = details.scroll.min(scroll_limit);
    }

    pub(super) fn scroll_package_details(&mut self, action: Action) {
        let details = &mut self.package_details;
        let current = details.scroll;
        let half_page = (details.viewport_rows / 2).max(1);
        let next = match action {
            Action::Up => current.saturating_sub(1),
            Action::Down => current.saturating_add(1),
            Action::HalfPageUp => current.saturating_sub(half_page),
            Action::HalfPageDown => current.saturating_add(half_page),
            Action::First => 0,
            Action::Last => details.scroll_limit,
            _ => return,
        };
        details.scroll = next.min(details.scroll_limit);
    }
}
