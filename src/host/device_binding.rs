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
        // not on the rest. Reported as Bound on the first, with the true
        // remainder standalone, so the sentence stays accurate.
        _ => Arrangement::Bound {
            host_translated: bound[0],
            standalone: all.into_iter().filter(|l| !bound.contains(l)).collect(),
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
            Self::AllBound { .. } | Self::Unbound { .. } | Self::OutOfRange { .. } => None,
        }
    }

    /// True when a host layer can fire at all.
    ///
    /// Matched on the variant, NOT on `host_translated().is_some()`. That
    /// spelling was the defect: `AllBound` has no single layer, so a knob
    /// flashed on all three reported that the daemon could not hear it.
    pub fn daemon_can_hear_the_knob(&self) -> bool {
        matches!(self, Self::Bound { .. } | Self::AllBound { .. })
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
mod tests {
    use super::*;
    use crate::device::DEVICE_LAYERS;

    #[test]
    fn one_bound_layer_leaves_the_others_standalone() {
        let a = arrangement(&[0], DEVICE_LAYERS);
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
            match arrangement(&[layer], DEVICE_LAYERS) {
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
        let a = arrangement(&[], DEVICE_LAYERS);
        assert_eq!(a.host_translated(), None);
        assert!(!a.daemon_can_hear_the_knob());
        let said = a.describe();
        assert!(said.contains("never hears"), "{said}");
        assert!(said.contains("no host layer can fire"), "{said}");
    }

    /// `describe` is read by a terminal AND by the settings app, so it must
    /// not carry either one's next step.
    ///
    /// This test used to assert the opposite -- that the unbound message
    /// contains "bind-slots" -- which pinned a shell command inside a string
    /// the GUI renders. It arrived there as literal backticks, directly
    /// under a button that runs exactly that command, beneath a warning that
    /// had already said the same thing. The assertion was holding the defect
    /// in place, so it is inverted rather than deleted.
    #[test]
    fn no_description_tells_the_reader_to_run_a_command() {
        let every = [
            arrangement(&[], DEVICE_LAYERS),
            arrangement(&[0], DEVICE_LAYERS),
            arrangement(&[DEVICE_LAYERS], DEVICE_LAYERS),
            arrangement(&[0], 1),
        ];
        for a in every {
            let said = a.describe();
            for shell in ["antiknob ", "bind-slots", "`", "--layer", "Run ", "Re-run"] {
                assert!(
                    !said.contains(shell),
                    "describe() names a command or shell syntax ({shell:?}): {said}"
                );
            }
            assert!(!said.is_empty());
        }
    }

    /// Clamping would hide a config that is wrong about something.
    #[test]
    fn a_layer_this_firmware_does_not_have_is_reported_not_clamped() {
        let a = arrangement(&[7], DEVICE_LAYERS);
        assert_eq!(
            a,
            Arrangement::OutOfRange {
                recorded: vec![7],
                device_layers: DEVICE_LAYERS,
            }
        );
        assert!(!a.daemon_can_hear_the_knob());
        assert!(a.describe().contains("only layers 1-3"), "{}", a.describe());
        // The last valid layer is still valid -- the boundary is exclusive.
        assert!(arrangement(&[DEVICE_LAYERS - 1], DEVICE_LAYERS).daemon_can_hear_the_knob());
    }

    /// Layers are 0-based in code and 1-based to a person, the same way the
    /// wire format is. Mixing them in a message is how a user binds the
    /// wrong layer twice.
    #[test]
    fn the_description_counts_layers_the_way_a_person_does() {
        let said = arrangement(&[1], DEVICE_LAYERS).describe();
        assert!(said.contains("Device layer 2 is host-translated"), "{said}");
        assert!(said.contains("1, 3"), "{said}");
        assert!(
            !said.contains("layer 0"),
            "0-based leaked into a message: {said}"
        );
    }

    #[test]
    fn a_single_layer_device_has_nothing_to_call_standalone() {
        let said = arrangement(&[0], 1).describe();
        assert!(said.contains("Device layer 1 is host-translated"), "{said}");
        assert!(!said.contains("standalone"), "{said}");
    }

    /// A config written before the set existed still loads, and means what
    /// it always meant.
    #[test]
    fn the_retired_single_layer_key_migrates_to_a_one_element_set() {
        let old = r#"{"layers":[{"name":"L"}],"boundDeviceLayer":1}"#;
        let cfg = super::super::HostConfig::try_load_json(old).expect("loads");
        assert_eq!(cfg.bound_device_layers, vec![1]);
        assert_eq!(
            arrangement(&cfg.bound_device_layers, DEVICE_LAYERS).host_translated(),
            Some(1)
        );
        // And it is never written back, so the file converges on one key.
        assert!(!cfg.to_json_pretty().contains("boundDeviceLayer\""));
        assert!(cfg.to_json_pretty().contains("boundDeviceLayers"));
    }

    /// `null` meant "nothing bound" and must not become `[0]`.
    #[test]
    fn a_null_single_layer_key_migrates_to_an_empty_set() {
        let old = r#"{"layers":[{"name":"L"}],"boundDeviceLayer":null}"#;
        let cfg = super::super::HostConfig::try_load_json(old).expect("loads");
        assert!(cfg.bound_device_layers.is_empty());
        assert!(!arrangement(&cfg.bound_device_layers, DEVICE_LAYERS).daemon_can_hear_the_knob());
    }

    /// A new config wins over a stale old key, rather than being overwritten
    /// by it -- the migration only fills a gap.
    #[test]
    fn the_new_key_wins_when_both_are_present() {
        let both = r#"{"layers":[{"name":"L"}],"boundDeviceLayer":0,"boundDeviceLayers":[0,1,2]}"#;
        let cfg = super::super::HostConfig::try_load_json(both).expect("loads");
        assert_eq!(cfg.bound_device_layers, vec![0, 1, 2]);
    }

    /// The state that had no representation at all.
    #[test]
    fn every_layer_bound_is_its_own_arrangement_not_unbound() {
        let a = arrangement(&[0, 1, 2], DEVICE_LAYERS);
        assert!(a.daemon_can_hear_the_knob(), "{a:?}");
        assert_eq!(a.host_translated(), None, "there is no single one");
        let said = a.describe();
        assert!(said.contains("All 3 device layers"), "{said}");
        assert!(
            !said.contains("no host layer can fire"),
            "a fully bound knob must not be described as unbound: {said}"
        );
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
        assert_eq!(record_bound_layers(&path, &[1]).unwrap(), vec![1]);
        let text = std::fs::read_to_string(&path).unwrap();
        let cfg = crate::host::HostConfig::load_json(&text);
        assert_eq!(cfg.bound_device_layers, vec![1]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Binding every layer RECORDS every layer.
    ///
    /// This test asserted the opposite -- that an all-layer bind records
    /// nothing, because there was "no single answer". That was true of the
    /// old `Option<u8>`, and it was the defect: the value written was
    /// indistinguishable from a knob that had never been flashed, so the app
    /// told a fully flashed knob no host layer could fire and the per-layer
    /// backlight never fired after the most common flash there is. The
    /// answer was not "no layer", it was "all of them" -- which the type
    /// could not say.
    #[test]
    fn binding_every_layer_records_every_layer() {
        let path = temp("all");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        assert_eq!(
            record_bound_layers(&path, &[0, 1, 2]).unwrap(),
            vec![0, 1, 2]
        );
        let back = super::super::HostConfig::try_load_json(
            &std::fs::read_to_string(&path).expect("written"),
        )
        .expect("readable");
        assert_eq!(back.bound_device_layers, vec![0, 1, 2]);
        assert!(
            arrangement(&back.bound_device_layers, DEVICE_LAYERS).daemon_can_hear_the_knob(),
            "a knob bound on every layer must read as one the daemon can hear"
        );
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

        record_bound_layers(&path, &[2]).unwrap();
        let cfg = crate::host::HostConfig::load_json(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(cfg.bound_device_layers, vec![2]);
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
        let err = record_bound_layers(&path, &[0]).expect_err("must refuse");
        assert!(err.to_string().contains("refusing to overwrite"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ this is not json",
            "the user's file was modified"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
