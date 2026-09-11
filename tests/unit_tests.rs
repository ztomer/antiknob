use antiknob::config::DeviceConfig;
use antiknob::presets::ALL_PRESETS;
use antiknob::protocol::{build_led_packet, key_id_for_button, key_id_for_knob, Action, KnobEvent};

#[test]
fn test_key_action_parsing() {
    let a = Action::parse("a").unwrap();
    assert_eq!(
        a,
        Action::Key {
            modifiers: 0,
            code: 0x04
        }
    );

    let combo = Action::parse("cmd+shift+z").unwrap();
    assert_eq!(
        combo,
        Action::Key {
            modifiers: 0x08 | 0x02,
            code: 0x1D,
        }
    );

    let space = Action::parse("space").unwrap();
    assert_eq!(
        space,
        Action::Key {
            modifiers: 0,
            code: 0x2C
        }
    );
}

#[test]
fn test_media_action_parsing() {
    let vol_up = Action::parse("volumeup").unwrap();
    assert_eq!(vol_up, Action::Media(0x00E9));

    let mute = Action::parse("mute").unwrap();
    assert_eq!(mute, Action::Media(0x00E2));

    let play = Action::parse("play").unwrap();
    assert_eq!(play, Action::Media(0x00CD));
}

#[test]
fn test_mouse_action_parsing() {
    let lclick = Action::parse("lclick").unwrap();
    assert_eq!(lclick, Action::MouseClick { button: 1 });

    let rclick = Action::parse("rclick").unwrap();
    assert_eq!(rclick, Action::MouseClick { button: 2 });

    let wheelup = Action::parse("wheelup").unwrap();
    assert_eq!(wheelup, Action::MouseWheel { delta: 1 });
}

#[test]
fn test_packet_structure() {
    let action = Action::parse("space").unwrap();
    let pkt = action.to_packet(key_id_for_button(0).unwrap(), 0);
    assert_eq!(pkt.len(), 64);
    assert_eq!(pkt[0], 0x03);
    // 0xFD, the encoder that writes an entry count. 0xFE records had none
    // and the firmware ran nothing from them.
    assert_eq!(pkt[1], 0xFD);
    assert_eq!(pkt[2], 0x01);
    assert_eq!(pkt[3], 0x01); // Layer 0 + 1
    assert_eq!(pkt[4], 0x01); // Keyboard kind
    assert_eq!(pkt[6], 1, "one entry: space has no modifiers");
    assert_eq!(pkt[9], 0x2C, "the HID usage for space, in entry 0");
}

#[test]
fn test_led_packet_generation() {
    let pkt = build_led_packet(0, "backlight white").unwrap();
    assert_eq!(pkt.len(), 64);
    assert_eq!(pkt[0], 0x03);
    assert_eq!(pkt[1], 0xFE);
    assert_eq!(pkt[2], 0xB0);
    assert_eq!(pkt[3], 0x00); // Layer 0
    assert_eq!(pkt[4], 0x01); // Mode 1
    assert_eq!(pkt[5], 255); // R
    assert_eq!(pkt[6], 255); // G
    assert_eq!(pkt[7], 255); // B

    let red_breathing = build_led_packet(1, "mode2 red").unwrap();
    assert_eq!(red_breathing[2], 0xB0);
    assert_eq!(red_breathing[3], 0x01); // Layer 1
    assert_eq!(red_breathing[4], 0x02); // Mode 2
    assert_eq!(red_breathing[5], 255); // Red R
    assert_eq!(red_breathing[6], 0); // Red G
}

/// This test used to pin the knob at 0x10/0x11/0x12 for every device, which
/// is what the code did and what the hardware does NOT do: a VK01 with three
/// buttons reads its knob from slots 4/5/6, so every binding written to 16-18
/// was accepted, stored, and ignored. The old expectations were part of the
/// defect, not a guard against it -- read back from the device, layer 3 held
/// our `cmd--`/`cmd-0`/`cmd-=` at 16/17/18 while the knob went on obeying
/// `volume up`/`prev`/`next` at 4/5/6.
#[test]
fn test_key_id_knob_and_buttons() {
    // Buttons are 1-based and unchanged.
    assert_eq!(key_id_for_button(0).unwrap(), 0x01);
    assert_eq!(key_id_for_button(1).unwrap(), 0x02);

    // The knob follows however many buttons the device declares.
    let kid = |b, e| key_id_for_knob(b, 0, e).unwrap();
    assert_eq!(kid(3, KnobEvent::RotateCCW), 4);
    assert_eq!(kid(3, KnobEvent::Press), 5);
    assert_eq!(kid(3, KnobEvent::RotateCW), 6);

    // A 15-key macropad is the layout the old constant happened to fit.
    assert_eq!(kid(15, KnobEvent::RotateCCW), 0x10);
    assert_eq!(kid(15, KnobEvent::Press), 0x11);
    assert_eq!(kid(15, KnobEvent::RotateCW), 0x12);
}

#[test]
fn test_all_workflow_presets_valid() {
    for preset in ALL_PRESETS {
        let ccw = Action::parse(preset.ccw);
        assert!(ccw.is_ok(), "Preset {} ccw action invalid", preset.id);

        let press = Action::parse(preset.press);
        assert!(press.is_ok(), "Preset {} press action invalid", preset.id);

        let cw = Action::parse(preset.cw);
        assert!(cw.is_ok(), "Preset {} cw action invalid", preset.id);

        let btn = Action::parse(preset.button);
        assert!(btn.is_ok(), "Preset {} button action invalid", preset.id);

        let led_spec = format!("mode{} {}", preset.led_mode, preset.led_color);
        let led_pkt = build_led_packet(0, &led_spec);
        assert!(led_pkt.is_ok(), "Preset {} led spec invalid", preset.id);
    }
}

#[test]
fn test_yaml_config_deserialization() {
    let yaml = r#"
model: Anticater VK01
orientation: normal
rows: 1
columns: 1
knobs: 1
layers:
  - buttons:
      - ["space"]
    knobs:
      - ccw: volumedown
        press: mute
        cw: volumeup
    led: "mode1 white"
"#;
    let cfg: DeviceConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.model, "Anticater VK01");
    assert_eq!(cfg.layers.len(), 1);
    assert_eq!(
        cfg.layers[0].knobs[0].ccw,
        Some(antiknob::binding::Binding::One("volumedown".to_string()))
    );
}
