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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TapHealth {
    pub active: bool,
    pub error: Option<String>,
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
