//! Application-wide state handed to every command.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use hd_runtime::harness::supervisor::Supervisor;

/// The one desktop link waiting for a frame to hand itself to.
///
/// A deep link that started the application arrives before any webview exists;
/// it is parked here and taken — once — by the first `desktop_offer`.
#[derive(Default)]
pub struct PendingLink {
    url: Mutex<Option<String>>,
}

impl PendingLink {
    pub fn park(&self, url: String) {
        *self.url.lock().expect("pending link poisoned") = Some(url);
    }

    pub fn take(&self) -> Option<String> {
        self.url.lock().expect("pending link poisoned").take()
    }
}

/// Application-wide state handed to every command.
pub struct AppState {
    pub supervisor: Arc<Supervisor>,
    /// Set while an install is running, so a second click cannot start another
    /// npm against the same directory.
    pub installing: AtomicBool,
    /// The number of harness boots this window has seen, for diagnostics.
    pub boots: AtomicU32,
    pub pending_link: PendingLink,
}

impl AppState {
    pub fn new(supervisor: Arc<Supervisor>) -> Self {
        Self {
            supervisor,
            installing: AtomicBool::new(false),
            boots: AtomicU32::new(0),
            pending_link: PendingLink::default(),
        }
    }
}
