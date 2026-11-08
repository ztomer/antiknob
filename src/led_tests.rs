use super::*;
use crate::firmware::LED_MODE_NAMES;

/// This knob ignores the colour bytes -- three different values all came
/// out red -- but the 16-key variant sharing its product id honours
/// them, so they must still be well-formed: base at 5-7, per-key from 8.
#[test]
fn layers_past_the_firmware_three_refuse_on_every_write_path() {
    // Every LED write (CLI led, led-probe, MCP set_led, upload, daemon
    // sync) builds its packet here, so the cap lives here.
    for layer in 0..crate::firmware::DEVICE_LAYERS {
        assert!(build_led_packet(layer, "green").is_ok(), "layer {layer}");
    }
    for layer in [3u8, 9, 15, 255] {
        let e = build_led_packet(layer, "green")
            .expect_err("must refuse")
            .to_string();
        assert!(e.contains("3 layers"), "layer {layer}: {e}");
    }
}

#[test]
fn the_base_colour_and_palette_sit_where_the_protocol_says() {
    let p = build_led_packet(0, "static blue").expect("packet");
    assert_eq!(&p[BASE_COLOR_OFFSET..BASE_COLOR_OFFSET + 3], &[0, 0, 255]);
    assert_eq!(&p[PALETTE_OFFSET..PALETTE_OFFSET + 3], &[0, 0, 255]);
}

#[test]
fn a_list_repeats_to_fill_the_palette() {
    let p = build_led_packet(0, "static red,blue").expect("packet");
    for i in 0..PALETTE_ENTRIES {
        let o = PALETTE_OFFSET + i * 3;
        let want: [u8; 3] = if i % 2 == 0 { [255, 0, 0] } else { [0, 0, 255] };
        assert_eq!(&p[o..o + 3], &want, "entry {i}");
    }
}

#[test]
fn an_unknown_colour_in_a_list_is_refused_rather_than_skipped() {
    assert!(build_led_packet(0, "static red,chartreuse").is_err());
    assert!(parse_palette("").is_err());
}

/// Mode 5 was refused for weeks as "crashes the firmware". It does not:
/// a capture of ANTICATER.app walking its own mode buttons shows
/// `03 FE B0 00 05` sent like any other mode, and the knob renders a
/// second multicoloured effect. This test used to pin the refusal, which
/// made it part of the defect rather than a guard against one.
#[test]
fn mode_five_builds_because_the_vendor_app_sends_it() {
    for spec in ["mode5", "custom", "rgb", "rgb rainbow"] {
        let p = build_led_packet(0, spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
        assert_eq!(p[4], 5, "{spec}");
        assert_eq!(&p[..4], &[0x03, 0xFE, 0xB0, 0x00]);
    }
}

/// Modes 1 and 2 are fixed COLOURS on this device, not the effects the
/// reference project's table names. The old names stay as aliases so
/// configs written against them keep loading.
#[test]
fn the_colour_names_and_their_legacy_aliases_reach_the_same_modes() {
    for (spec, mode) in [
        ("red", 1u8),
        ("static", 1),
        ("backlight", 1),
        ("green", 2),
        ("reactive", 2),
        ("shock", 2),
        ("rainbow", 4),
        ("press", 4),
        ("rgb", 5),
        ("custom", 5),
    ] {
        let p = build_led_packet(0, spec).unwrap_or_else(|e| panic!("{spec}: {e}"));
        assert_eq!(p[4], mode, "{spec}");
    }
}

/// Every mode the device has must be nameable, or `led-read` reports a
/// live mode as unknown and `led-probe` cannot walk them all.
#[test]
fn every_mode_the_device_has_is_named_and_buildable() {
    assert_eq!(LED_MODE_NAMES.len(), 6);
    for (mode, name) in LED_MODE_NAMES.iter().enumerate() {
        let p = build_led_packet(0, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(p[4] as usize, mode, "{name}");
    }
}

/// The number mapping agrees with the packet builder everywhere they
/// overlap: canonical names hit their own index, aliases hit the same
/// mode the packet carries, and unknown words are `None` rather than a
/// mode that would read back as a mismatch. `sync_led` trusts this to
/// skip writes, so a wrong entry here silently drops a light change.
#[test]
fn mode_number_names_its_index() {
    for (mode, name) in LED_MODE_NAMES.iter().enumerate() {
        assert_eq!(
            led_mode_number(name),
            Some(u8::try_from(mode).unwrap()),
            "{name}"
        );
    }
    for (spec, mode) in [
        ("static", 1u8),
        ("backlight", 1),
        ("steady", 1),
        ("mode1", 1),
        ("reactive", 2),
        ("shock", 2),
        ("mode2", 2),
        ("shock2", 3),
        ("mode3", 3),
        ("press", 4),
        ("mode4", 4),
        ("custom", 5),
        ("mode5", 5),
        ("mode0", 0),
        ("red white", 1),
        ("  GREEN  ", 2),
    ] {
        assert_eq!(led_mode_number(spec), Some(mode), "{spec}");
    }
    for spec in ["", "   ", "chartreuse", "mode6", "reddish"] {
        assert_eq!(led_mode_number(spec), None, "{spec}");
    }
}

/// Every LED write needs this in front or the device stores the mode,
/// reads it back, and changes nothing.
#[test]
fn the_init_packet_is_the_one_the_firmware_requires() {
    let p = led_init_packet();
    assert_eq!(&p[..4], &[0x03, 0xFB, 0xFB, 0xFB]);
    assert_eq!(p.len(), 64);
}

#[test]
fn the_mode_table_matches_the_hardware_tested_one() {
    // Named for what the knob shows, not for the reference project's
    // effect names: mode 1 is red and mode 2 is green on this device.
    assert_eq!(
        LED_MODE_NAMES,
        ["off", "red", "green", "ripple", "rainbow", "rgb"]
    );
    for (name, want) in [
        ("off", 0u8),
        ("red", 1),
        ("green", 2),
        ("ripple", 3),
        ("rainbow", 4),
        ("rgb", 5),
    ] {
        let p = build_led_packet(0, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(p[4], want, "{name}");
    }
}

// ---- per-key colours (`<mode> keys=...`) ------------------------------

fn sixteen(list: &str) -> String {
    vec![list; 16].join(",")
}

/// The vendor writes its 16 triplets from byte 5, right after the mode,
/// with no base colour in front; the rest of the report is zero.
#[test]
fn keys_are_written_from_byte_five_in_the_vendor_layout() {
    let spec = format!(
        "rgb keys={}",
        (0..16)
            .map(|i| format!("{i:02x}{:02x}ff", 0xF0 - i))
            .collect::<Vec<_>>()
            .join(",")
    );
    let p = build_led_packet(1, &spec).expect("sixteen colours");
    assert_eq!(&p[..5], &[0x03, 0xFE, 0xB0, 1, 5]);
    for i in 0..16usize {
        let i8_ = u8::try_from(i).unwrap();
        assert_eq!(
            &p[5 + i * 3..8 + i * 3],
            &[i8_, 0xF0 - i8_, 0xFF],
            "entry {i}"
        );
    }
    assert!(p[53..].iter().all(|b| *b == 0), "nothing past the 48 bytes");
    assert_eq!(p.len(), 64);
}

#[test]
fn keys_vendor_is_the_palette_the_vendor_binary_holds() {
    let p = build_led_packet(0, "rainbow keys=vendor").unwrap();
    assert_eq!(&p[5..8], &[0xFF, 0x00, 0x00]);
    assert_eq!(&p[8..11], &[0xFF, 0x80, 0x30]);
    assert_eq!(&p[50..53], &[0xFF, 0xC8, 0x64]);
    assert_eq!(p[4], 4);
}

#[test]
fn a_hash_prefix_and_upper_case_are_accepted() {
    let a = build_led_packet(0, &format!("red keys={}", sixteen("#FF8030"))).unwrap();
    let b = build_led_packet(0, &format!("red keys={}", sixteen("ff8030"))).unwrap();
    assert_eq!(a, b);
}

/// The reading pins 16 triplets and nothing else: no padding, no
/// repeating, no named colours beside them.
#[test]
fn what_the_reading_does_not_pin_is_refused() {
    let fifteen = vec!["ff0000"; 15].join(",");
    let seventeen = vec!["ff0000"; 17].join(",");
    for spec in [
        "rgb keys=".to_string(),
        "rgb keys=ff0000".to_string(),
        format!("rgb keys={fifteen}"),
        format!("rgb keys={seventeen}"),
        format!("rgb keys={}", sixteen("ff00")),
        format!("rgb keys={}", sixteen("ff00000")),
        format!("rgb keys={}", sixteen("gg0000")),
        format!("rgb keys={}", sixteen("red")),
        format!("rgb keys={}", sixteen("+f0000")),
        format!("rgb red keys={}", sixteen("ff0000")),
        format!("rgb keys={} extra", sixteen("ff0000")),
        "chartreuse keys=vendor".to_string(),
        "keys=vendor".to_string(),
    ] {
        assert!(build_led_packet(0, &spec).is_err(), "{spec}");
    }
    // The layer cap still applies on the per-key path.
    assert!(build_led_packet(3, "rgb keys=vendor").is_err());
}

/// A stored mode says nothing about stored colours, so a per-key write
/// is never skipped for "already holds that mode".
#[test]
fn a_per_key_write_is_never_skipped_on_a_matching_mode() {
    assert_eq!(skippable_mode_number("rgb"), Some(5));
    assert_eq!(skippable_mode_number("rgb keys=vendor"), None);
    assert_eq!(skippable_mode_number("off keys=vendor"), None);
}

/// Specs without `keys=` are byte-for-byte what they were.
#[test]
fn specs_without_keys_keep_their_layout() {
    let p = build_led_packet(0, "static blue").unwrap();
    assert_eq!(&p[BASE_COLOR_OFFSET..BASE_COLOR_OFFSET + 3], &[0, 0, 255]);
    assert_eq!(&p[PALETTE_OFFSET..PALETTE_OFFSET + 3], &[0, 0, 255]);
}
