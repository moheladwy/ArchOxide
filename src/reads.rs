use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::{
    appimage, flatpak, fonts,
    packages::{self, Inventory, SearchResult, SearchSource},
};

const FONT_SLOT: usize = 3;
const FLATPAK_INVENTORY_SLOT: usize = 4;
const FLATPAK_SEARCH_SLOT: usize = 5;
const IMAGE_INVENTORY_SLOT: usize = 6;

pub const SEARCH_SOURCES: [SearchSource; 2] = [SearchSource::Repository, SearchSource::Aur];

pub fn search_index(source: SearchSource) -> usize {
    match source {
        SearchSource::Repository => 0,
        SearchSource::Aur => 1,
    }
}

#[derive(Clone, Debug)]
pub enum Request {
    Inventory(u64),
    Search(SearchSource, u64, String),
    Fonts(u64),
    FlatpakInventory(u64),
    FlatpakSearch(u64, String),
    ImageInventory(u64),
}

impl Request {
    fn slot(&self) -> usize {
        match self {
            Self::Inventory(_) => 0,
            Self::Search(source, ..) => search_index(*source) + 1,
            Self::Fonts(_) => FONT_SLOT,
            Self::FlatpakInventory(_) => FLATPAK_INVENTORY_SLOT,
            Self::FlatpakSearch(..) => FLATPAK_SEARCH_SLOT,
            Self::ImageInventory(_) => IMAGE_INVENTORY_SLOT,
        }
    }

    fn run(self) -> Response {
        match self {
            Self::Inventory(id) => Response::Inventory(id, packages::installed()),
            Self::Search(source, id, query) => {
                Response::Search(source, id, packages::search(source, &query))
            }
            Self::Fonts(id) => Response::Fonts(id, fonts::installed_families()),
            Self::FlatpakInventory(id) => Response::FlatpakInventory(id, flatpak::installed()),
            Self::FlatpakSearch(id, query) => Response::FlatpakSearch(id, flatpak::search(&query)),
            Self::ImageInventory(id) => Response::ImageInventory(id, appimage::installed()),
        }
    }

    fn failed(self, error: String) -> Response {
        match self {
            Self::Inventory(id) => Response::Inventory(id, Err(error)),
            Self::Search(source, id, _) => Response::Search(source, id, Err(error)),
            Self::Fonts(id) => Response::Fonts(id, Err(error)),
            Self::FlatpakInventory(id) => Response::FlatpakInventory(id, Err(error)),
            Self::FlatpakSearch(id, _) => Response::FlatpakSearch(id, Err(error)),
            Self::ImageInventory(id) => Response::ImageInventory(id, Err(error)),
        }
    }
}

#[derive(Debug)]
pub enum Response {
    Inventory(u64, Result<Inventory, String>),
    Search(SearchSource, u64, Result<Vec<SearchResult>, String>),
    Fonts(u64, Result<Vec<String>, String>),
    FlatpakInventory(u64, Result<Vec<flatpak::InstalledApp>, String>),
    FlatpakSearch(u64, Result<Vec<flatpak::SearchApp>, String>),
    ImageInventory(u64, Result<Vec<appimage::Entry>, String>),
}

impl Response {
    fn slot(&self) -> usize {
        match self {
            Self::Inventory(..) => 0,
            Self::Search(source, ..) => search_index(*source) + 1,
            Self::Fonts(..) => FONT_SLOT,
            Self::FlatpakInventory(..) => FLATPAK_INVENTORY_SLOT,
            Self::FlatpakSearch(..) => FLATPAK_SEARCH_SLOT,
            Self::ImageInventory(..) => IMAGE_INVENTORY_SLOT,
        }
    }
}

#[derive(Debug, Default)]
struct Slot {
    active: bool,
    queued: Option<Request>,
}

#[derive(Debug)]
pub struct Reads {
    slots: [Slot; IMAGE_INVENTORY_SLOT + 1],
    sender: Sender<Response>,
    receiver: Receiver<Response>,
}

impl Default for Reads {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            slots: Default::default(),
            sender,
            receiver,
        }
    }
}

impl Reads {
    pub fn submit(&mut self, request: Request) {
        let slot = &mut self.slots[request.slot()];
        if slot.active {
            slot.queued = Some(request);
            return;
        }

        slot.active = true;
        let sender = self.sender.clone();
        let work = request.clone();
        if let Err(error) = thread::Builder::new().spawn(move || {
            let _ = sender.send(work.run());
        }) {
            let _ = self.sender.send(request.failed(error.to_string()));
        }
    }

    pub fn clear_queued_searches(&mut self) {
        for slot in &mut self.slots[1..=SEARCH_SOURCES.len()] {
            slot.queued = None;
        }
    }

    pub fn clear_flatpak_search(&mut self) {
        self.slots[FLATPAK_SEARCH_SLOT].queued = None;
    }

    pub fn clear_flatpak_inventory(&mut self) {
        self.slots[FLATPAK_INVENTORY_SLOT].queued = None;
    }

    pub fn clear_image_inventory(&mut self) {
        self.slots[IMAGE_INVENTORY_SLOT].queued = None;
    }

    pub fn receive(&mut self) -> Option<Response> {
        let response = self.receiver.try_recv().ok()?;
        let slot = &mut self.slots[response.slot()];
        slot.active = false;
        if let Some(request) = slot.queued.take() {
            self.submit(request);
        }
        Some(response)
    }

    pub fn pending(&self) -> bool {
        self.slots.iter().any(|slot| slot.active)
    }
}
