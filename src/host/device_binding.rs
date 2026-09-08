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
    /// A layer was recorded that this firmware does not have. Reported
    /// rather than clamped: a config naming layer 7 is wrong about
    /// something, and quietly reading it as layer 2 would hide that.
    OutOfRange { recorded: u8, device_layers: u8 },
}

/// Work out the arrangement from the recorded layer and the firmware's count.
pub fn arrangement(bound: Option<u8>, device_layers: u8) -> Arrangement {
    let all: Vec<u8> = (0..device_layers).collect();
    match bound {
        None => Arrangement::Unbound { device_layers: all },
        Some(layer) if layer >= device_layers => Arrangement::OutOfRange {
            recorded: layer,
            device_layers,
        },
        Some(layer) => Arrangement::Bound {
            host_translated: layer,
            standalone: all.into_iter().filter(|l| *l != layer).collect(),
        },
    }
}

impl Arrangement {
    /// The device layer the daemon hears, if any.
    pub fn host_translated(&self) -> Option<u8> {
        match self {
            Self::Bound {
                host_translated, ..
            } => Some(*host_translated),
            Self::Unbound { .. } | Self::OutOfRange { .. } => None,
        }
    }

    /// True when a host layer can fire at all.
    pub fn daemon_can_hear_the_knob(&self) -> bool {
        self.host_translated().is_some()
    }

    /// One line for a person, saying what is live and what is not.
    ///
    /// Always says WHY when something cannot work, because the failure it
    /// describes is otherwise invisible: a virtual layer that never fires
    /// looks exactly like one that is configured wrong.
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
            Self::Unbound { .. } => "No device layer is bound to slot chords, so the daemon \
                 never hears the knob and no host layer can fire. Run \
                 `antiknob bind-slots --layer N` to bind one."
                .to_string(),
            Self::OutOfRange {
                recorded,
                device_layers,
            } => format!(
                "Device layer {} is recorded as host-translated, but this firmware has \
                 only layers 1-{}. Re-run `antiknob bind-slots --layer N`.",
                recorded + 1,
                device_layers
            ),
        }
    }
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
pub fn record_bound_layer(path: &Path, layers: &[u8]) -> Result<Option<u8>> {
    if layers.len() != 1 {
        return Ok(None);
    }
    let layer = layers[0];
    let mut cfg = match std::fs::read_to_string(path) {
        Ok(text) => super::HostConfig::try_load_json(&text).ok_or_else(|| {
            anyhow::anyhow!(
                "{} exists but is not readable host config; refusing to overwrite it",
                path.display()
            )
        })?,
        Err(_) => super::HostConfig::default_config(),
    };
    cfg.bound_device_layer = Some(layer);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, cfg.to_json_pretty())
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(Some(layer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DEVICE_LAYERS;

    #[test]
    fn one_bound_layer_leaves_the_others_standalone() {
        let a = arrangement(Some(0), DEVICE_LAYERS);
        assert_eq!(a.host_translated(), Some(0));
        assert_eq!(
            a,
            Arrangement::Bound {
                host_translated: 0,
                standalone: vec![1, 2],
            }
        );
        assert!(a.daemon_can_hear_the_knob());
    }

    #[test]
    fn the_bound_layer_is_never_also_listed_as_standalone() {
        for layer in 0..DEVICE_LAYERS {
            match arrangement(Some(layer), DEVICE_LAYERS) {
                Arrangement::Bound {
                    host_translated,
                    standalone,
                } => {
                    assert_eq!(host_translated, layer);
                    assert!(
                        !standalone.contains(&layer),
                        "layer {layer} is both host-translated and standalone"
                    );
                    assert_eq!(standalone.len() as u8, DEVICE_LAYERS - 1);
                }
                other => panic!("layer {layer} should be bound, got {other:?}"),
            }
        }
    }

    /// The failure this module exists for. A virtual layer with nothing
    /// bound never fires, and before this said so there was nothing to
    /// search for -- no error, no banner, just a knob that did nothing.
    #[test]
    fn nothing_bound_is_reported_as_a_reason_not_as_a_default_layer() {
        let a = arrangement(None, DEVICE_LAYERS);
        assert_eq!(a.host_translated(), None);
        assert!(!a.daemon_can_hear_the_knob());
        let said = a.describe();
        assert!(said.contains("never hears"), "{said}");
        assert!(said.contains("bind-slots"), "{said}");
    }

    /// Clamping would hide a config that is wrong about something.
    #[test]
    fn a_layer_this_firmware_does_not_have_is_reported_not_clamped() {
        let a = arrangement(Some(7), DEVICE_LAYERS);
        assert_eq!(
            a,
            Arrangement::OutOfRange {
                recorded: 7,
                device_layers: DEVICE_LAYERS,
            }
        );
        assert!(!a.daemon_can_hear_the_knob());
        assert!(a.describe().contains("only layers 1-3"), "{}", a.describe());
        // The last valid layer is still valid -- the boundary is exclusive.
        assert!(arrangement(Some(DEVICE_LAYERS - 1), DEVICE_LAYERS).daemon_can_hear_the_knob());
    }

    /// Layers are 0-based in code and 1-based to a person, the same way the
    /// wire format is. Mixing them in a message is how a user binds the
    /// wrong layer twice.
    #[test]
    fn the_description_counts_layers_the_way_a_person_does() {
        let said = arrangement(Some(1), DEVICE_LAYERS).describe();
        assert!(said.contains("Device layer 2 is host-translated"), "{said}");
        assert!(said.contains("1, 3"), "{said}");
        assert!(
            !said.contains("layer 0"),
            "0-based leaked into a message: {said}"
        );
    }

    #[test]
    fn a_single_layer_device_has_nothing_to_call_standalone() {
        let said = arrangement(Some(0), 1).describe();
        assert!(said.contains("Device layer 1 is host-translated"), "{said}");
        assert!(!said.contains("standalone"), "{said}");
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "antiknob-db-{}-{}/host.json",
            std::process::id(),
            tag
        ))
    }

    #[test]
    fn recording_one_layer_creates_the_config_and_survives_a_reload() {
        let path = temp("create");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(record_bound_layer(&path, &[1]).unwrap(), Some(1));
        let text = std::fs::read_to_string(&path).unwrap();
        let cfg = crate::host::HostConfig::load_json(&text);
        assert_eq!(cfg.bound_device_layer, Some(1));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Binding every layer leaves no single answer, so nothing is recorded.
    /// Picking one would put a claim in the config that is wrong two thirds
    /// of the time.
    #[test]
    fn binding_every_layer_records_nothing_rather_than_guessing() {
        let path = temp("all");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(record_bound_layer(&path, &[0, 1, 2]).unwrap(), None);
        assert!(!path.exists(), "an ambiguous bind must not write a claim");
        assert_eq!(record_bound_layer(&path, &[]).unwrap(), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The user's other settings have to survive; this writes one field.
    #[test]
    fn recording_preserves_the_rest_of_the_config() {
        let path = temp("preserve");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut original = crate::host::HostConfig::default_config();
        original.scroll_lines_per_detent = 11;
        original.double_tap_switch = false;
        std::fs::write(&path, original.to_json_pretty()).unwrap();

        record_bound_layer(&path, &[2]).unwrap();
        let cfg = crate::host::HostConfig::load_json(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(cfg.bound_device_layer, Some(2));
        assert_eq!(cfg.scroll_lines_per_detent, 11);
        assert!(!cfg.double_tap_switch);
        assert_eq!(cfg.layers.len(), original.layers.len());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// A config that exists but will not parse is somebody's file. Replacing
    /// it with defaults to record one field would throw their layers away.
    #[test]
    fn an_unparseable_config_is_refused_rather_than_overwritten() {
        let path = temp("garbage");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ this is not json").unwrap();
        let err = record_bound_layer(&path, &[0]).expect_err("must refuse");
        assert!(err.to_string().contains("refusing to overwrite"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ this is not json",
            "the user's file was modified"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
