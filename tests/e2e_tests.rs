use antiknob::config::DeviceConfig;
use antiknob::protocol::{build_led_packet, key_id_for_button, key_id_for_knob, Action, KnobEvent};
use std::fs;
use std::path::Path;

/// The YAML round trip that `antiknob upload` depends on: a config written
/// out and read back must survive unchanged. Previously routed through the
/// egui GUI's profile helpers; now exercised against the shipping loader.
#[test]
fn test_e2e_config_roundtrip_profile() {
    let temp_dir = std::env::temp_dir().join("antiknob_test_profiles");
    let test_file = temp_dir.join("profile_test.yaml");
    fs::create_dir_all(&temp_dir).unwrap();

    let original_yaml = r#"
model: Anticater VK01
orientation: normal
rows: 1
columns: 1
knobs: 1
layers:
  - buttons:
      - ["play"]
    knobs:
      - ccw: volumedown
        press: mute
        cw: volumeup
    led: "mode1 white"
  - buttons:
      - ["cmd+b"]
    knobs:
      - ccw: left
        press: space
        cw: right
    led: "mode2 red"
"#;
    let original: DeviceConfig = serde_yaml::from_str(original_yaml).unwrap();
    fs::write(&test_file, serde_yaml::to_string(&original).unwrap()).unwrap();

    let loaded = DeviceConfig::load_from_file(&test_file).unwrap();
    assert_eq!(loaded.model, original.model);
    assert_eq!(loaded.layers.len(), 2);
    assert_eq!(loaded.layers[1].buttons[0][0], "cmd+b");

    let _ = fs::remove_file(test_file);
    let _ = fs::remove_dir(temp_dir);
}

#[test]
fn test_e2e_packet_generation_pipeline() {
    let yaml = r#"
model: Anticater VK01
orientation: normal
rows: 1
columns: 1
knobs: 1
layers:
  - buttons:
      - ["cmd+shift+z"]
    knobs:
      - ccw: volumedown
        press: mute
        cw: volumeup
    led: "mode3 blue"
"#;
    let config: DeviceConfig = serde_yaml::from_str(yaml).unwrap();
    let layer = &config.layers[0];

    // Verify knob packets
    let ccw_action = Action::parse(&layer.knobs[0].ccw.as_ref().unwrap().names()[0]).unwrap();
    let buttons: usize = layer.buttons.iter().map(Vec::len).sum();
    let ccw_id = key_id_for_knob(buttons, 0, KnobEvent::RotateCCW).unwrap();
    let ccw_pkt = ccw_action.to_packet(ccw_id, 0);
    assert_eq!(ccw_pkt[0], 0x03);
    // 0xFD: keyboard and media single actions share the sequence encoder,
    // the only one that writes an entry count. The 0xFE records this test
    // used to assert were stored by the device and never executed.
    assert_eq!(ccw_pkt[1], 0xFD);
    assert_eq!(ccw_pkt[2], ccw_id);
    assert_eq!(ccw_pkt[3], 0x01); // Layer 0 + 1
    assert!(ccw_pkt[6] > 0, "the record must declare what to run");

    // Verify button packet
    let btn_action = Action::parse(&layer.buttons[0][0]).unwrap();
    let btn_id = key_id_for_button(0).unwrap();
    let btn_pkt = btn_action.to_packet(btn_id, 0);
    assert_eq!(btn_pkt[0], 0x03);
    assert_eq!(btn_pkt[1], 0xFD);
    assert_eq!(btn_pkt[2], btn_id);
    assert_eq!(btn_pkt[3], 0x01); // Layer 0 + 1
    assert_eq!(btn_pkt[4], 0x01); // Keyboard kind
    assert!(btn_pkt[6] > 0, "the record must declare what to run");

    // Verify LED packet
    let led_pkt = build_led_packet(0, layer.led.as_ref().unwrap()).unwrap();
    assert_eq!(led_pkt[0], 0x03);
    assert_eq!(led_pkt[1], 0xFE);
    assert_eq!(led_pkt[2], 0xB0);
    assert_eq!(led_pkt[3], 0x00); // Layer 0
    assert_eq!(led_pkt[4], 0x03); // Mode 3
    assert_eq!(led_pkt[5], 0); // Blue R
    assert_eq!(led_pkt[6], 0); // Blue G
    assert_eq!(led_pkt[7], 255); // Blue B
}

#[test]
fn test_e2e_invalid_action_syntax_rejected() {
    assert!(Action::parse("invalid_unknown_key_xyz").is_err());
    assert!(Action::parse("cmd+invalid_key").is_err());
    assert!(Action::parse("").is_err());
}

#[test]
fn test_e2e_invalid_led_syntax_rejected() {
    assert!(build_led_packet(0, "unknown_mode").is_err());
    // Mode 5 is ALLOWED, reversing the refusal this test used to pin.
    //
    // The refusal was right to resist the argument it was resisting. "The
    // vendor app sends it" is not "it is safe to send", and that inference
    // had been made and reverted twice. It is not the evidence now: the
    // OWNER OF THE DEVICE reported watching mode 5 render a second
    // multicoloured effect on this knob. An observation of the hardware
    // outranks an inference about the hardware, in both directions.
    //
    // What most likely happened the night mode 5 was blamed: the known
    // freeze bug (kriomant/ch57x-keyboard-tool#175) strikes 2s-2m after ANY
    // mode change and needs a replug. A mode change followed by a frozen
    // renderer is exactly what that looks like, and mode 5 was the mode
    // being changed to at the time.
    //
    // If a mode-5 write is ever seen to wedge a knob again, restore the
    // refusal -- but record what distinguished that run from this report,
    // because "it crashed once" is what was believed for weeks.
    for spec in ["mode5", "custom", "rgb"] {
        let p = build_led_packet(0, spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
        assert_eq!(p[4], 5, "{spec}");
    }
    // The multicoloured effect is mode 4.
    assert_eq!(build_led_packet(0, "rainbow").expect("rainbow")[4], 4);
    assert!(build_led_packet(0, "mode1 ultraviolet").is_err());
}

#[test]
fn test_e2e_cli_validate_bundled_config() {
    let config_path = Path::new("config.yaml");
    assert!(config_path.exists(), "config.yaml must exist in root");

    let content = fs::read_to_string(config_path).unwrap();
    let config: DeviceConfig = serde_yaml::from_str(&content).unwrap();
    assert_eq!(config.layers.len(), 3);

    for (layer_idx, layer) in config.layers.iter().enumerate() {
        for knob in &layer.knobs {
            // Every gesture, whichever shape it takes in the YAML. A
            // sequence validates as a whole -- it has to fit one record and
            // its actions have to be one kind -- so this asks the binding
            // rather than parsing a string it may not be.
            for binding in [&knob.ccw, &knob.press, &knob.cw].into_iter().flatten() {
                binding.validate().expect("knob binding");
            }
        }
        for row in &layer.buttons {
            for btn in row {
                if btn != "none" && !btn.is_empty() {
                    assert!(Action::parse(btn).is_ok());
                }
            }
        }
        if let Some(ref led) = layer.led {
            assert!(build_led_packet(layer_idx as u8, led).is_ok());
        }
    }
}
