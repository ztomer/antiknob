//! The Swift app and the Rust daemon both model `host.json`. Nothing made them
//! agree, and they had drifted.
//!
//! `tools/check_swift_rust_model_parity.py` is the gate: it reads the serde
//! attributes on the Rust side and the `CodingKeys`/`encode(to:)` on the Swift
//! side and fails when they disagree. This file is that gate's PROOF, plus the
//! one thing a static comparison cannot do — hand the bytes to the real
//! deserializer and insist nothing was lost on the way in.
//!
//! ## Why the drift was invisible, and how bad it was
//!
//! `tests/surface_parity.rs` closes the CLI/MCP version of this hole, and says
//! nothing about Swift: the two surfaces are generated from one table, and
//! Swift is a hand-written second model in another language that is never
//! type-checked against the first. Measured before the fix, by feeding each
//! spelling to the real `serde`:
//!
//! | the app wrote                   | the daemon reads | what happened |
//! |---------------------------------|------------------|---------------|
//! | `{"type":"openURL"}`            | `openUrl`        | file UNDECODABLE → `load_json` returned DEFAULTS, every layer lost |
//! | `{"type":"launchApp","bundleId":…}` | `bundle_id`  | same |
//! | `{"delayMs":40}`                | `delay_ms`       | parsed, delay read back `None`: the pause silently dropped |
//! | (no `variants` field at all)    | `variants`       | a GUI save deleted every virtual layer |
//!
//! The first two are total, silent loss: `try_load_json` returns `None`,
//! `load_json` substitutes the defaults, and the user's configuration is
//! replaced by two starter layers with nothing anywhere saying so.

use antiknob::host::virtual_layer::LayerVariant;
use antiknob::host::{HostAction, HostConfig, HostLayer};

// ── The real deserializer, on the bytes the app actually writes ──────────────
//
// The static gate compares NAMES. This half proves the consequence: the
// daemon's real `serde` accepts a whole `host.json` of the shape the app
// produces, with every value intact. A name can agree while the bytes still
// fail to decode — `delayMs` parses fine and yields `None`, which is the
// whole reason that drift survived a test suite.

/// The complete set of things an app-written layer can carry.
///
/// Every one of these was a live disagreement; they are written out here in
/// full rather than sampled so that a NEW `HostAction` variant, which this
/// test cannot know about, is caught by the static gate instead of quietly
/// going untested by this one.
#[test]
fn every_action_the_app_can_encode_decodes_and_keeps_its_value() {
    let cases: Vec<(&str, HostAction)> = vec![
        (
            r#"{"type":"openUrl","url":"https://example.invalid"}"#,
            HostAction::OpenUrl {
                url: "https://example.invalid".into(),
            },
        ),
        (
            r#"{"type":"launchApp","bundle_id":"com.apple.Safari"}"#,
            HostAction::LaunchApp {
                bundle_id: "com.apple.Safari".into(),
            },
        ),
        (
            r#"{"type":"quitApp","bundle_id":"com.apple.Safari","force":true}"#,
            HostAction::QuitApp {
                bundle_id: "com.apple.Safari".into(),
                force: true,
            },
        ),
        (
            r#"{"type":"keyChord","key":43,"mods":["cmd"],"label":","}"#,
            HostAction::KeyChord {
                key: 43,
                mods: vec!["cmd".into()],
                label: ",".into(),
            },
        ),
        (
            r#"{"type":"aux","key":"volumeUp"}"#,
            HostAction::Aux {
                key: antiknob::host::AuxKey::VolumeUp,
            },
        ),
        (
            r#"{"type":"mouseClick","button":"middle"}"#,
            HostAction::MouseClick {
                button: antiknob::host::MouseButton::Middle,
            },
        ),
        (
            r#"{"type":"scroll","lines":-3}"#,
            HostAction::Scroll { lines: Some(-3) },
        ),
        (
            r#"{"type":"openPath","path":"/tmp/x"}"#,
            HostAction::OpenPath {
                path: "/tmp/x".into(),
            },
        ),
    ];
    for (json, want) in cases {
        let got: HostAction = serde_json::from_str(json)
            .unwrap_or_else(|e| panic!("the daemon cannot read what the app writes: {json}\n{e}"));
        assert_eq!(got, want, "{json}");
    }
}

/// The `delayMs` arm, which parses and still loses the value.
///
/// A test that only asserted "it decodes" would pass on the broken spelling:
/// serde ignores the unknown key, so `{"delayMs":40}` is a perfectly good
/// `SeqStep` with `delay_ms: None`. The assertion has to be on the VALUE.
#[test]
fn a_sequence_delay_survives_the_wire() {
    let snake: antiknob::host::SeqStep =
        serde_json::from_str(r#"{"key":8,"mods":["cmd"],"label":"C","delay_ms":40}"#)
            .expect("the app writes delay_ms");
    assert_eq!(snake.delay_ms, Some(40), "the inter-step pause was dropped");

    // And the spelling that silently loses it, named as the defect it is. If
    // this ever starts decoding to Some, the app's older files are readable
    // again and this assertion should be revisited -- not deleted.
    let camel: antiknob::host::SeqStep =
        serde_json::from_str(r#"{"key":8,"mods":["cmd"],"label":"C","delayMs":40}"#)
            .expect("an unknown key is ignored, not refused");
    assert_eq!(
        camel.delay_ms, None,
        "`delayMs` now decodes; if that is intended, update the app's encoder's \
         comment, which says it does not"
    );
}

/// A whole `host.json` as the app writes it, virtual layers included.
///
/// This is the shape the gate's `variants` arm exists for: before the app had
/// the field, saving from the GUI deleted every variant, and the layers went
/// inert with nothing to search for.
#[test]
fn an_app_written_config_survives_with_its_virtual_layers() {
    let json = r#"{
      "layers": [
        {
          "name": "Navigate",
          "twistL": {"type": "scroll", "lines": -3},
          "twistR": {"type": "aux", "key": "volumeUp"},
          "holdTwistL": {"type": "keyChord", "key": 43, "mods": ["cmd"], "label": ","},
          "holdTwistR": {"type": "none"},
          "press": {"type": "openUrl", "url": "https://example.invalid"},
          "led": "rainbow",
          "variants": [
            {
              "name": "Safari",
              "apps": ["com.apple.Safari"],
              "twistL": {"type": "launchApp", "bundle_id": "com.apple.Safari"},
              "twistR": {"type": "none"},
              "holdTwistL": {"type": "none"},
              "holdTwistR": {"type": "none"},
              "press": {"type": "none"}
            }
          ]
        }
      ],
      "doubleTapSwitch": true,
      "doubleTapWindow": 0.25,
      "scrollLinesPerDetent": 3,
      "boundDeviceLayers": [0, 1, 2]
    }"#;

    let cfg = HostConfig::try_load_json(json).expect(
        "the daemon must read what the app writes; load_json would silently \
                 substitute the defaults, losing every layer",
    );

    assert_eq!(cfg.layers.len(), 1);
    let layer = &cfg.layers[0];
    assert_eq!(layer.name, "Navigate");
    assert_eq!(layer.led.as_deref(), Some("rainbow"));
    assert!(layer.is_virtual(), "the app dropped the layer's variants");
    assert_eq!(layer.variants.len(), 1);
    let v: &LayerVariant = &layer.variants[0];
    assert_eq!(v.name, "Safari");
    assert_eq!(v.apps, vec!["com.apple.Safari".to_string()]);
    assert_eq!(
        v.twist_l,
        HostAction::LaunchApp {
            bundle_id: "com.apple.Safari".into()
        }
    );
    assert_eq!(cfg.bound_device_layers, vec![0, 1, 2]);

    // And re-serialising keeps the variants, which is the half that used to
    // destroy them: reading was never the problem, WRITING was.
    let again = cfg.to_json_pretty();
    assert!(
        again.contains("\"variants\""),
        "a load/save cycle dropped the variants: {again}"
    );
    assert!(
        again.contains("com.apple.Safari"),
        "a load/save cycle dropped the variant's apps"
    );
}

/// Re-encoding must not invent a `variants` key for a fixed layer.
///
/// The daemon uses `skip_serializing_if = "Vec::is_empty"`, so an empty array
/// is noise the file did not have before. Round-tripping has to be a fixed
/// point for a layer with no variants, or every save grows the file.
#[test]
fn a_fixed_layer_does_not_gain_an_empty_variants_key() {
    let layer = HostLayer::empty("Plain");
    let text = serde_json::to_string(&layer).expect("serialise");
    assert!(!text.contains("variants"), "{text}");
}
