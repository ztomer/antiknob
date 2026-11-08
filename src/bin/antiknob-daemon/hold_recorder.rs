//! The hold instrument's writer thread (AK.HOLDMEASURE).
//!
//! The grab-tap callback must never touch the disk -- a slow callback is how
//! the system disables a tap -- so it only sends samples down a channel; this
//! thread owns the file. What a sample is, and the bound on the file, are in
//! `antiknob::host::hold_stats`.

use antiknob::host::hold_stats::{self, HoldSample};
use antiknob::policy::HOLD_LOG_CAP_BYTES;
use goh_tui::WARN;
use std::sync::mpsc::{channel, Sender};

/// Start the writer and return the sending half.
pub fn spawn() -> Sender<HoldSample> {
    let (tx, rx) = channel::<HoldSample>();
    std::thread::spawn(move || {
        let path = match hold_stats::default_path() {
            Ok(p) => p,
            Err(e) => {
                goh_tui::outln!("{WARN} Hold instrument off: {e}");
                return;
            }
        };
        let mut warned = false;
        for sample in rx {
            if let Err(e) = hold_stats::append(&path, sample, HOLD_LOG_CAP_BYTES) {
                if !warned {
                    warned = true;
                    goh_tui::outln!(
                        "{WARN} Hold instrument cannot write {}: {e}",
                        path.display()
                    );
                }
            }
        }
    });
    tx
}

/// Hand a drained batch to the writer. A writer that is gone (it could not
/// find its directory) drops them: the instrument is optional, the tap is not.
pub fn send_all(tx: &Sender<HoldSample>, samples: Vec<HoldSample>) {
    for sample in samples {
        let _ = tx.send(sample);
    }
}
