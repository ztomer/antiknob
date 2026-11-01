//! Tests for `device_binding`, split out for the file-length cap.

use super::*;
use crate::firmware::DEVICE_LAYERS;

/// CLASS: `describe` names EVERY bound layer, for a bound subset of any
/// size. A subset of two was reported as "Bound on the first", so the
/// sentence named layer 1 and silently dropped layer 2.
#[test]
fn describe_names_every_bound_layer_for_subsets_of_one_two_and_many() {
    let cases: [(&[u8], u8); 4] = [(&[1], 3), (&[0, 2], 3), (&[0, 1, 3], 5), (&[1, 2, 3, 4], 6)];
    for (bound, n) in cases {
        let a = arrangement(bound, n);
        let said = a.describe();
        for l in bound {
            assert!(
                said.contains(&(l + 1).to_string()),
                "layer {} missing: {said}",
                l + 1
            );
        }
        for l in (0..n).filter(|l| !bound.contains(l)) {
            assert!(
                said.contains(&(l + 1).to_string()),
                "standalone {} missing: {said}",
                l + 1
            );
        }
        assert!(a.daemon_can_hear_the_knob(), "{said}");
        let one = bound.len() == 1;
        assert_eq!(a.host_translated().is_some(), one, "{a:?}");
    }
}

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
        assert_ne!(said.len(), 0);
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
    assert_eq!(cfg.bound_device_layers.len(), 0);
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
    let back =
        super::super::HostConfig::try_load_json(&std::fs::read_to_string(&path).expect("written"))
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
