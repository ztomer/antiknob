//! What a command executes AGAINST: the config path, the tap engine, and
//! the tap's health.
//!
//! Split from `dispatch.rs` for the file-length cap, along the seam between
//! the thing being dispatched to and the dispatching. Nothing here runs a
//! command; `dispatch` holds no state of its own.

use crate::host::tap::TapEngine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Tap health status for diagnostics.
///
/// `active` alone is NOT health, and reporting it as though it were cost a
/// whole debugging session. In grab mode it is set true immediately before
/// the blocking `keytap::grab` call and false only when that call RETURNS,
/// so it means "we are inside the tap" and not "the tap receives events". A
/// tap the window server created and then never fed reads as perfectly
/// healthy forever -- which is exactly the failure `keytap` documents as the
/// one a tap cannot report about itself.
///
/// `events_seen` is the missing half. It is the count of key events the tap
/// has actually been handed, so silence becomes VISIBLE: `active: true` with
/// `events_seen: 0` after the user has typed says "deaf", and no amount of
/// staring at `active` ever could.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TapHealth {
    pub active: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub events_seen: u64,
}

/// Execution context for API commands.
pub struct ApiContext {
    pub config_path: PathBuf,
    pub tap_engine: Option<Arc<Mutex<TapEngine>>>,
    pub tap_health: Arc<Mutex<TapHealth>>,
}

impl ApiContext {
    pub fn new(config_path: PathBuf, tap_engine: Option<Arc<Mutex<TapEngine>>>) -> Self {
        Self {
            config_path,
            tap_engine,
            tap_health: Arc::new(Mutex::new(TapHealth::default())),
        }
    }

    pub fn with_health(
        config_path: PathBuf,
        tap_engine: Option<Arc<Mutex<TapEngine>>>,
        tap_health: Arc<Mutex<TapHealth>>,
    ) -> Self {
        Self {
            config_path,
            tap_engine,
            tap_health,
        }
    }
}
