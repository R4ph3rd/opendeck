//! The main thread's table of live plugin instances.
//!
//! Thread-local: an instance (not `Send`) is created by a main-thread job
//! and stays here until its effect or instrument is dropped. Ids are unique
//! per process.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::backend::{InstanceRequests, ParamChange, PluginInstance};
use crate::{InstanceStatus, PluginError};

/// Process-unique instance id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(pub u64);

/// One live instance and the main-thread ends of its parameter rings.
pub(crate) struct Entry {
    pub instance: Box<dyn PluginInstance>,
    /// Editor and state changes going to the audio half.
    pub to_audio: rtrb::Producer<ParamChange>,
    /// Changes the plugin made, coming from the audio half.
    pub from_audio: rtrb::Consumer<ParamChange>,
    pub status: InstanceStatus,
    /// Requests the plugin made, until the control thread takes them.
    pub requests: InstanceRequests,
}

thread_local! {
    static TABLE: RefCell<HashMap<InstanceId, Entry>> = RefCell::new(HashMap::new());
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Stores an entry. Main thread.
pub(crate) fn insert(entry: Entry) -> InstanceId {
    let id = InstanceId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
    TABLE.with(|t| t.borrow_mut().insert(id, entry));
    id
}

/// Runs `f` on the entry. Main thread. Plugin callbacks reached from `f`
/// must not re-enter the table (backends queue host requests instead).
pub(crate) fn with<T>(id: InstanceId, f: impl FnOnce(&mut Entry) -> T) -> Result<T, PluginError> {
    TABLE.with(|t| {
        t.borrow_mut()
            .get_mut(&id)
            .map(f)
            .ok_or(PluginError::UnknownInstance(id.0))
    })
}

/// Removes and returns the entry. Main thread.
pub(crate) fn remove(id: InstanceId) -> Option<Entry> {
    TABLE.with(|t| t.borrow_mut().remove(&id))
}

/// Services every live instance (plugin main-thread callbacks) and keeps
/// what they ask of the host in their entries. Main thread.
pub(crate) fn service_all() {
    TABLE.with(|t| {
        for entry in t.borrow_mut().values_mut() {
            let asked = entry.instance.service();
            entry.requests = entry.requests.merge(asked);
        }
    });
}
