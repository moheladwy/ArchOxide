use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::packages::{self, Inventory, SearchResult, SearchSource};

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
}

impl Request {
    fn slot(&self) -> usize {
        match self {
            Self::Inventory(_) => 0,
            Self::Search(source, ..) => search_index(*source) + 1,
        }
    }

    fn run(self) -> Response {
        match self {
            Self::Inventory(id) => Response::Inventory(id, packages::installed()),
            Self::Search(source, id, query) => {
                Response::Search(source, id, packages::search(source, &query))
            }
        }
    }

    fn failed(self, error: String) -> Response {
        match self {
            Self::Inventory(id) => Response::Inventory(id, Err(error)),
            Self::Search(source, id, _) => Response::Search(source, id, Err(error)),
        }
    }
}

#[derive(Debug)]
pub enum Response {
    Inventory(u64, Result<Inventory, String>),
    Search(SearchSource, u64, Result<Vec<SearchResult>, String>),
}

impl Response {
    fn slot(&self) -> usize {
        match self {
            Self::Inventory(..) => 0,
            Self::Search(source, ..) => search_index(*source) + 1,
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
    slots: [Slot; 3],
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
        for slot in &mut self.slots[1..] {
            slot.queued = None;
        }
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
