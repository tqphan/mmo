//! The subscription director: keeps one tier's window of cells subscribed around the player
//! (PLAN §4.3, docs/design/interest.md). The Godot client mirrors it.
//!
//! Each cell is its own subscription, so moving the window fetches only the cells that came into
//! it. Cells that left are dropped only once every cell of the new window is applied, so there's
//! never a gap. The window follows the player with hysteresis (`sim::cell::recenter`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use spacetimedb_sdk::{DbContext, SubscriptionHandle as _};

use crate::module_bindings::*;

pub struct Director {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    tables: &'static [&'static str],
    radius: u16,
    center: Option<u64>,
    wanted: HashSet<u64>,
    held: HashMap<u64, Held>,
    recenters: u32,
}

struct Held {
    handle: SubscriptionHandle,
    applied: bool,
}

impl Inner {
    /// Drop every held cell outside the window, once the whole window is applied.
    fn retire(&mut self) {
        if !self.wanted.iter().all(|c| self.held.get(c).is_some_and(|h| h.applied)) {
            return;
        }
        let wanted = &self.wanted;
        for (_, h) in self.held.extract_if(|c, _| !wanted.contains(c)) {
            let _ = h.handle.unsubscribe();
        }
    }
}

impl Director {
    /// A window of `(2·radius+1)²` cells over `tables`, each of which has a `cell` column.
    pub fn new(tables: &'static [&'static str], radius: u16) -> Self {
        let inner = Inner {
            tables,
            radius,
            center: None,
            wanted: HashSet::new(),
            held: HashMap::new(),
            recenters: 0,
        };
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    /// Call with the player's predicted position after every move. Returns whether the window moved.
    pub fn update(&self, conn: &DbConnection, x: f32, z: f32) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let center = match inner.center {
            None => sim::cell::cell_of(sim::cell::OPEN_WORLD, x, z),
            Some(c) => sim::cell::recenter(c, x, z, sim::cell::HYSTERESIS_M),
        };
        if inner.center == Some(center) {
            return false;
        }
        if inner.center.is_some() {
            inner.recenters += 1;
        }
        inner.center = Some(center);
        inner.wanted = sim::cell::window_cells(center, inner.radius).into_iter().collect();
        let new: Vec<u64> = inner.wanted.iter().copied().filter(|c| !inner.held.contains_key(c)).collect();
        for cell in new {
            let queries: Vec<String> =
                inner.tables.iter().map(|t| format!("SELECT * FROM {t} WHERE cell = {cell}")).collect();
            let shared = self.inner.clone();
            // The callback can't run before we insert below: it needs the lock we hold.
            let handle = conn
                .subscription_builder()
                .on_applied(move |_| {
                    let mut inner = shared.lock().unwrap();
                    if let Some(h) = inner.held.get_mut(&cell) {
                        h.applied = true;
                    }
                    inner.retire();
                })
                .on_error(move |_, e| eprintln!("subscription error for cell {cell}: {e}"))
                .subscribe(queries);
            inner.held.insert(cell, Held { handle, applied: false });
        }
        // Coming back to cells still held can complete the window straight away.
        inner.retire();
        true
    }

    /// Whether every cell of the current window is applied.
    pub fn ready(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.center.is_some() && inner.wanted.iter().all(|c| inner.held.get(c).is_some_and(|h| h.applied))
    }

    /// Whether the window is applied and nothing outside it is still held.
    pub fn settled(&self) -> bool {
        self.ready() && {
            let inner = self.inner.lock().unwrap();
            inner.held.len() == inner.wanted.len()
        }
    }

    /// How many times the window has moved since the first `update`.
    pub fn recenters(&self) -> u32 {
        self.inner.lock().unwrap().recenters
    }

    pub fn center(&self) -> Option<u64> {
        self.inner.lock().unwrap().center
    }
}
