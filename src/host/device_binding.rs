//! Which DEVICE layer the daemon actually hears from.
//!
//! The firmware's three layers and the host's three layers are different
//! things that share a word. A firmware layer is a fixed keymap the device
//! runs on its own; a host layer is what the daemon does when it sees the
//! slot chords. The daemon only ever hears the ONE firmware layer that has
//! been bound to those chords -- the other two run standalone and the daemon
//! is not involved in them at all.
//!
//! Nothing recorded that. `bind-slots` flashed a layer and forgot; the
//! daemon translated whatever chords arrived without knowing where from; and
//! the GUI presented three host layers as though all three were live. The
//! failure that produces is silent and total: configure a virtual layer,
//! never run `bind-slots`, and the layer simply never fires. No error, no
//! banner, nothing to search for -- the exact shape the house rule about
//! honest placeholders exists to forbid.
//!
//! So the arrangement is recorded and reported. This module is the pure half:
//! given which layer was bound and how many the firmware has, say what the
//! user actually has.
//!
//! This knob cannot switch device layers -- it has no spare button, and a
//! 512-query read-only sweep found nothing that reports or sets the active
//! one -- so the arrangement is chosen once and stays. See PLAN.md item 3.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What the daemon can and cannot see, given what was flashed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Arrangement {
    /// No device layer sends slot chords, so the daemon hears nothing and
    /// every host layer is inert however carefully it is configured.
    Unbound { device_layers: Vec<u8> },
    /// One device layer carries the chords. The rest run standalone: real
    /// hardware layers that work with the daemon stopped.
    Bound {
        host_translated: u8,
        standalone: Vec<u8>,
    },
    /// EVERY device layer sends the chords, so the daemon hears the knob
    /// whichever layer it is on and no layer runs standalone.
    ///
    /// This is what `bind-slots` with no `--layer` produces, and it had no
    /// representation: `Option<u8>` cannot say "all of them", so it was
    /// recorded as `None` -- the same value as "nothing is bound". The app
    /// then told a fully flashed knob that no host layer could fire, two
    /// lines under its own reading of the firmware saying host-translate.
    AllBound { device_layers: Vec<u8> },
    /// Two or more device layers carry the chords, but not all of them.
    ///
    /// This used to be reported as `Bound` on the first of them, and the
    /// sentence then named that one layer and dropped the rest.
    SomeBound {
        host_translated: Vec<u8>,
        standalone: Vec<u8>,
    },
    /// A layer was recorded that this firmware does not have. Reported
    /// rather than clamped: a config naming layer 7 is wrong about
    /// something, and quietly reading it as layer 2 would hide that.
    OutOfRange {
        recorded: Vec<u8>,
        device_layers: u8,
    },
}

/// Work out the arrangement from the recorded SET and the firmware's count.
pub fn arrangement(bound: &[u8], device_layers: u8) -> Arrangement {
    let all: Vec<u8> = (0..device_layers).collect();
    let mut bound: Vec<u8> = bound.to_vec();
    bound.sort_unstable();
    bound.dedup();

    if bound.iter().any(|l| *l >= device_layers) {
        return Arrangement::OutOfRange {
            recorded: bound,
            device_layers,
        };
    }
    match bound.len() {
        0 => Arrangement::Unbound { device_layers: all },
        // Only when there is more than one layer to bind: on a
        // single-layer firmware "all of them" and "that one" are the same
        // arrangement, and naming the layer is the more useful sentence.
        n if n == all.len() && all.len() > 1 => Arrangement::AllBound { device_layers: all },
        1 => Arrangement::Bound {
            host_translated: bound[0],
            standalone: all.into_iter().filter(|l| *l != bound[0]).collect(),
        },
        // A subset larger than one: the daemon hears the knob on those and
        // not on the rest, and every one of them is named.
        _ => Arrangement::SomeBound {
            standalone: all.into_iter().filter(|l| !bound.contains(l)).collect(),
            host_translated: bound,
        },
    }
}

impl Arrangement {
    /// The one device layer the daemon hears, when there IS just one.
    ///
    /// `None` for `AllBound` too, which is why it must not be used to decide
    /// whether the daemon can hear the knob -- see below.
    pub fn host_translated(&self) -> Option<u8> {
        match self {
            Self::Bound {
                host_translated, ..
            } => Some(*host_translated),
            Self::AllBound { .. }
            | Self::SomeBound { .. }
            | Self::Unbound { .. }
            | Self::OutOfRange { .. } => None,
        }
    }

    /// True when a host layer can fire at all.
    ///
    /// Matched on the variant, NOT on `host_translated().is_some()`. That
    /// spelling was the defect: `AllBound` has no single layer, so a knob
    /// flashed on all three reported that the daemon could not hear it.
    pub fn daemon_can_hear_the_knob(&self) -> bool {
        matches!(
            self,
            Self::Bound { .. } | Self::SomeBound { .. } | Self::AllBound { .. }
        )
    }

    /// One line for a person, saying what is live and what is not.
    ///
    /// Always says WHY when something cannot work, because the failure it
    /// describes is otherwise invisible: a virtual layer that never fires
    /// looks exactly like one that is configured wrong.
    ///
    /// States the FACT and stops there. Each failing case used to end with
    /// "Run `antiknob bind-slots --layer N`", which is the right next step
    /// in a terminal and the wrong one in the settings app -- where it drew
    /// literal backticks under a button that does exactly that. One string
    /// cannot carry both calls to action, so it carries neither and each
    /// surface adds its own.
    pub fn describe(&self) -> String {
        match self {
            Self::Bound {
                host_translated,
                standalone,
            } => {
                let others: Vec<String> = standalone.iter().map(|l| (l + 1).to_string()).collect();
                if others.is_empty() {
                    format!("Device layer {} is host-translated.", host_translated + 1)
                } else {
                    format!(
                        "Device layer {} is host-translated; layer(s) {} run standalone \
                         and are unaffected by the daemon.",
                        host_translated + 1,
                        others.join(", ")
                    )
                }
            }
            Self::SomeBound {
                host_translated,
                standalone,
            } => format!(
                "Device layers {} are host-translated; layer(s) {} run standalone \
                 and are unaffected by the daemon.",
                one_based(host_translated),
                one_based(standalone)
            ),
            Self::AllBound { device_layers } => format!(
                "All {} device layers send the slot chords, so the daemon hears the knob \
                 whichever layer it is on.",
                device_layers.len()
            ),
            Self::Unbound { .. } => "No device layer is bound to slot chords, so the daemon \
                 never hears the knob and no host layer can fire."
                .to_string(),
            Self::OutOfRange {
                recorded,
                device_layers,
            } => format!(
                "Device layer(s) {} are recorded as host-translated, but this firmware \
                 has only layers 1-{}.",
                recorded
                    .iter()
                    .map(|l| (l + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                device_layers
            ),
        }
    }
}

/// Layer indexes as a person counts them: `[0, 2]` -> "1, 3".
fn one_based(layers: &[u8]) -> String {
    layers
        .iter()
        .map(|l| (l + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Record which device layer now carries the slot chords.
///
/// Takes the path rather than resolving `HOME` itself, so the write is
/// testable without a real home directory -- the version of this that read
/// the environment lived in the binary and nothing could reach it.
///
/// A single layer is recorded. Binding several would leave the record
/// ambiguous, and on this hardware it is also moot: the knob sits on one
/// device layer and cannot switch, so only one can ever be heard. Flashing
/// all three is still useful -- whichever layer the knob is on then works --
/// so that case records NOTHING and returns `None` rather than picking one
/// and being wrong about it.
///
/// An unreadable or absent config is replaced with the defaults rather than
/// failing: the flash has already landed by the time this runs, and losing
/// the record is better than reporting a successful flash as an error. A
/// config that exists but cannot be PARSED is a different matter and is not
/// silently overwritten -- see `HostConfig::try_load_json`.
pub fn record_bound_layers(path: &Path, layers: &[u8]) -> Result<Vec<u8>> {
    let mut cfg = match std::fs::read_to_string(path) {
        Ok(text) => super::HostConfig::try_load_json(&text).ok_or_else(|| {
            anyhow::anyhow!(
                "{} exists but is not readable host config; refusing to overwrite it",
                path.display()
            )
        })?,
        Err(_) => super::HostConfig::default_config(),
    };
    let mut recorded: Vec<u8> = layers.to_vec();
    recorded.sort_unstable();
    recorded.dedup();
    cfg.bound_device_layers = recorded.clone();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::host::config_backup::write_with_backup(path, &cfg.to_json_pretty())
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(recorded)
}

#[cfg(test)]
#[path = "device_binding_tests.rs"]
mod tests;
