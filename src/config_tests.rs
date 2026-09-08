//! Tests for `config`, split out under the 500-line file cap.
//!
//! This file IS the body of `config`'s `mod tests`, pulled in with `#[path]`
//! so the tests keep reaching `config`'s private items -- the same shape
//! `host` uses for `engine_virtual_tests`. Indentation is left exactly as it
//! was: these tests embed YAML in raw strings, and de-indenting the file
//! silently re-indents the YAML inside them.

use super::*;

fn cfg(rows: usize, columns: usize, buttons: Vec<Vec<&str>>, knobs: usize) -> DeviceConfig {
    DeviceConfig {
        model: default_model(),
        orientation: default_orientation(),
        rows,
        columns,
        knobs,
        layers: vec![LayerConfig {
            buttons: buttons
                .into_iter()
                .map(|r| r.into_iter().map(str::to_string).collect())
                .collect(),
            knobs: (0..knobs)
                .map(|_| KnobConfig {
                    ccw: Some(Binding::One("volumedown".to_string())),
                    press: Some(Binding::One("mute".to_string())),
                    cw: Some(Binding::One("volumeup".to_string())),
                    hold_twist_l: None,
                    hold_twist_r: None,
                })
                .collect(),
            led: None,
        }],
    }
}

/// The layout is the only thing that says where knob slots land, so the
/// count it yields is what `key_id_for_knob` is handed.
fn temp_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("antiknob-cfg-{}-{}", std::process::id(), tag))
}

#[test]
fn the_default_layout_path_sits_beside_the_daemons_own_state() {
    let p = default_device_config_path(Path::new("/Users/x"));
    assert_eq!(
        p,
        PathBuf::from("/Users/x/Library/Application Support/antiknob/config.yaml")
    );
}

#[test]
fn a_missing_layout_is_seeded_and_an_existing_one_is_never_touched() {
    let home = temp_home("seed");
    let _ = fs::remove_dir_all(&home);

    let path = resolve_device_config_path(&home, None).expect("seed");
    assert_eq!(path, default_device_config_path(&home));
    assert!(path.exists(), "first use must write the starter layout");
    assert!(DeviceConfig::load_from_file(&path).is_ok());

    // The user's edits survive a second resolve, however unusual.
    fs::write(
        &path,
        "model: ch57x-1
rows: 0
columns: 0
knobs: 1
layers: []
",
    )
    .unwrap();
    let again = resolve_device_config_path(&home, None).expect("resolve");
    let body = fs::read_to_string(&again).unwrap();
    assert!(
        body.contains("layers: []"),
        "an existing config was overwritten"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_explicit_path_wins_and_seeds_nothing() {
    let home = temp_home("explicit");
    let _ = fs::remove_dir_all(&home);
    let given = PathBuf::from("/tmp/somewhere/else.yaml");
    assert_eq!(
        resolve_device_config_path(&home, Some(given.clone())).unwrap(),
        given
    );
    assert!(
        !default_device_config_path(&home).exists(),
        "an explicit path must not seed the default one"
    );
    let _ = fs::remove_dir_all(&home);
}

/// The compiled-in starter has to be a layout the tool accepts, or first
/// use writes a file that immediately fails to load.
#[test]
fn the_packaged_starter_layout_is_valid() {
    let cfg: DeviceConfig = serde_yaml::from_str(STARTER_CONFIG).expect("starter parses");
    cfg.validate().expect("starter validates");
    // ONE button, because keys 2-6 are all gestures and there is no room
    // for more. Declaring three pushed every gesture two slots along.
    assert_eq!(cfg.button_count(), 1);
    assert_eq!(cfg.slots_per_layer(), 6, "one button plus five gestures");
    // And the starter binds every gesture the knob has, including the
    // two this repo spent months saying did not exist.
    let knob = &cfg.layers[0].knobs[0];
    assert_eq!(knob.bindings().len(), 5, "all five gestures are bound");
    assert!(knob.hold_twist_l.is_some());
    assert!(knob.hold_twist_r.is_some());
}

/// The layer LEDs are the user's way of telling layers apart at a
/// glance, so the starter has to actually carry them -- and carry the
/// colours asked for, not whatever survived an edit.
#[test]
fn the_starter_layout_gives_each_layer_its_own_effect() {
    let cfg: DeviceConfig = serde_yaml::from_str(STARTER_CONFIG).expect("starter parses");
    // Layers are told apart by COLOUR. The note this replaces said that
    // was impossible because the knob has one colour -- it does not:
    // mode 1 is red and mode 2 is green, which is why setting mode 1 to
    // three different colours produced red three times.
    assert_eq!(cfg.layers[0].led.as_deref(), Some("red"));
    assert_eq!(cfg.layers[1].led.as_deref(), Some("green"));
    let a = crate::protocol::build_led_packet(0, "red").expect("red");
    assert_eq!(&a[2..5], &[0xB0, 0x00, 0x01]);
    let b = crate::protocol::build_led_packet(1, "green").expect("green");
    assert_eq!(&b[2..5], &[0xB0, 0x01, 0x02]);
}

/// Mode 4 is the multicoloured effect the device ships in. Mode 5 is a
/// second one and is no longer refused -- the vendor app sends it.
#[test]
fn the_third_layer_uses_the_multicoloured_mode() {
    let cfg: DeviceConfig = serde_yaml::from_str(STARTER_CONFIG).expect("starter parses");
    assert_eq!(cfg.layers[2].led.as_deref(), Some("rainbow"));
    let packet = crate::protocol::build_led_packet(2, "rainbow").expect("rainbow packet");
    assert_eq!(&packet[2..5], &[0xB0, 0x02, 0x04]);
    assert_eq!(
        crate::protocol::build_led_packet(2, "mode5").expect("mode5")[4],
        5
    );
}

#[test]
fn the_button_count_comes_from_the_declared_grid() {
    // VK01: one row of three keys.
    assert_eq!(
        cfg(1, 3, vec![vec!["play", "prev", "next"]], 1).button_count(),
        3
    );
    // The 15-key macropad the old hardcoded base happened to fit.
    assert_eq!(cfg(3, 5, vec![], 1).button_count(), 15);
    assert_eq!(cfg(0, 0, vec![], 1).button_count(), 0);
}

/// A layer binding more buttons than the grid declares pushes the extra
/// one into the knob's first slot, where it is accepted and ignored.
#[test]
fn a_layer_with_more_buttons_than_the_grid_is_refused() {
    let bad = cfg(1, 2, vec![vec!["play", "prev", "next"]], 1);
    let err = bad
        .validate()
        .expect_err("3 buttons in a 1x2 grid must be refused");
    let msg = err.to_string();
    assert!(msg.contains("lists 3"), "{msg}");
    assert!(msg.contains("key ID 3"), "{msg}");

    // The same bindings in the grid that fits are fine.
    assert!(cfg(1, 3, vec![vec!["play", "prev", "next"]], 1)
        .validate()
        .is_ok());
}

/// `rows: 0` puts knob slots at key IDs 1/2/3. Harmless with no keys,
/// destructive with them -- the caller has to confirm which device it is.
#[test]
fn a_zero_button_layout_is_flagged_because_knob_slots_start_at_key_one() {
    assert!(cfg(0, 0, vec![], 1).knob_slots_start_at_the_first_button());
    // Buttons declared: knob slots start after them, no confirmation needed.
    assert!(
        !cfg(1, 3, vec![vec!["play", "prev", "next"]], 1).knob_slots_start_at_the_first_button()
    );
    // No knobs at all: nothing to collide.
    assert!(!cfg(0, 0, vec![], 0).knob_slots_start_at_the_first_button());
}

/// A zero-button layout that still lists buttons is self-contradictory
/// and never valid, with or without the --knob-only confirmation.
#[test]
fn a_zero_button_layout_that_binds_buttons_is_always_refused() {
    let err = cfg(0, 0, vec![vec!["play"]], 1)
        .validate()
        .expect_err("binding buttons a layout does not declare must be refused");
    let msg = err.to_string();
    assert!(msg.contains("lists 1 button(s)"), "{msg}");
    assert!(msg.contains("declares 0"), "{msg}");
}

/// The three YAML shapes a gesture can take. A plain string keeps the
/// meaning it has always had; the list and map forms are new.
#[test]
fn a_gesture_accepts_a_string_a_list_or_a_timed_map() {
    let y = r#"
model: ch57x-1
rows: 1
columns: 3
knobs: 1
layers:
  - buttons: [["play", "prev", "next"]]
    knobs:
      - ccw: "volumedown"
        press: ["cmd-c", "cmd-v"]
        cw:
          steps: ["cmd-a", "cmd-c"]
          delay_ms: 120
"#;
    let cfg: DeviceConfig = serde_yaml::from_str(y).expect("parses");
    cfg.validate().expect("valid");
    let knob = &cfg.layers[0].knobs[0];

    let ccw = knob.ccw.as_ref().unwrap();
    // Still "not a sequence" as far as the CONFIG is concerned -- but
    // it no longer rides a separate encoder. The 0xFE single-action
    // path wrote records with no entry count, so a bare string flashed
    // a slot the firmware would not run; one encoder now serves both.
    assert!(!ccw.is_sequence(), "a bare string is still a single action");
    let p = ccw.to_packet(4, 0).unwrap();
    assert_eq!(p[1], 0xFD);
    assert!(p[6] > 0, "a bare string must still declare its entries");

    let press = knob.press.as_ref().unwrap();
    assert!(press.is_sequence());
    assert_eq!(press.to_packet(5, 0).unwrap()[1], 0xFD);
    assert_eq!(press.delay_ms(), 0);

    let cw = knob.cw.as_ref().unwrap();
    assert_eq!(cw.delay_ms(), 120);
    let p = cw.to_packet(6, 0).unwrap();
    assert_eq!(p[1], 0xFD);
    // cmd-a then cmd-c = four entries; the delay sits on the SECOND
    // step, not before the first.
    assert_eq!(p[6], 4);
    assert_eq!(&p[7..10], &[0, 0, 0xF4], "no wait before the opening chord");
    assert_eq!(&p[13..16], &[0, 120, 0xF4], "120ms before the second");
}

/// A one-element list is still the sequence form. Downgrading it would
/// make `steps: [x]` and `x` behave differently from how they read.
#[test]
fn a_single_element_list_is_still_a_sequence() {
    let b: Binding = serde_yaml::from_str("[\"cmd-c\"]").unwrap();
    assert!(b.is_sequence());
    assert_eq!(b.to_packet(4, 0).unwrap()[1], 0xFD);
}

/// Load time is where a binding the device cannot store should fail --
/// not at flash time, with half the layers already written.
#[test]
fn a_binding_the_device_cannot_store_is_refused_when_it_loads() {
    // Media cannot chain: the firmware keeps one action per slot.
    let media: Binding = serde_yaml::from_str("[\"volumeup\", \"next\"]").unwrap();
    let err = media.validate().expect_err("a media chain must be refused");
    assert!(err.to_string().contains("one media action"), "{err}");

    // Twenty entries do not fit in one record.
    let long: Vec<String> = (0..20).map(|_| "a".to_string()).collect();
    let err = Binding::Sequence(long).validate().expect_err("too long");
    assert!(err.to_string().contains("entries"), "{err}");

    // An empty sequence binds nothing.
    assert!(Binding::Sequence(vec![]).validate().is_err());

    // And an unknown action name is still caught.
    assert!(Binding::One("chartreuse".into()).validate().is_err());
}

/// The whole point: a gesture can type a string.
#[test]
fn a_gesture_can_type_a_sequence_of_keystrokes() {
    let b = Binding::Sequence(
        ["h", "e", "l", "l", "o"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    b.validate().expect("valid");
    let p = b.to_packet(2, 0).unwrap();
    assert_eq!(p[4], 1, "keyboard kind");
    assert_eq!(p[6], 5, "five entries");
    assert_eq!(p[9], 0x0B, "h");
    assert_eq!(p[12], 0x08, "e");
    assert_eq!(p[15], 0x0F, "l");
}
