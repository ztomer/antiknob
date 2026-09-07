//! A layer whose bindings change with context.
//!
//! Media and Navigate are fixed: five gestures, always the same five
//! actions. The third layer is meant to mean different things in different
//! places -- one thing in a browser, another in an editor -- and to be
//! settable by an agent over MCP as readily as by a person.
//!
//! Both drivers land on the same question: given some context, which set of
//! bindings is live? That question is pure, and it is the whole of this
//! module. Asking macOS what is frontmost is somebody else's job (see
//! `frontmost`), so every rule here is testable without a window server.

use super::{Gesture, HostAction, HostLayer};
use serde::{Deserialize, Serialize};

/// One named alternative set of bindings for a virtual layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerVariant {
    pub name: String,
    /// Bundle ids this variant claims, e.g. `com.apple.Safari`. Empty
    /// claims nothing: a variant with no apps is reachable only by an
    /// explicit override, which is a legitimate way to keep a manual-only
    /// set alongside automatic ones.
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default, alias = "twist_l")]
    pub twist_l: HostAction,
    #[serde(default, alias = "twist_r")]
    pub twist_r: HostAction,
    #[serde(default, alias = "hold_twist_l")]
    pub hold_twist_l: HostAction,
    #[serde(default, alias = "hold_twist_r")]
    pub hold_twist_r: HostAction,
    #[serde(default)]
    pub press: HostAction,
}

impl LayerVariant {
    pub fn action(&self, gesture: Gesture) -> &HostAction {
        match gesture {
            Gesture::TwistL => &self.twist_l,
            Gesture::Press => &self.press,
            Gesture::TwistR => &self.twist_r,
            Gesture::HoldTwistL => &self.hold_twist_l,
            Gesture::HoldTwistR => &self.hold_twist_r,
        }
    }

    pub fn claims(&self, bundle_id: &str) -> bool {
        self.apps.iter().any(|a| a == bundle_id)
    }
}

/// What the daemon knows when it has to choose a variant.
///
/// Both fields are read at the moment a gesture fires, never cached: the
/// frontmost app at completion time is a different app from the one the
/// user was looking at when they turned the knob.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwitchContext {
    /// Bundle id of the frontmost application, if it could be read.
    pub frontmost: Option<String>,
    /// A variant pinned by hand or by an agent. Outranks the app.
    pub override_name: Option<String>,
}

/// Why a particular set of bindings is live. Reported over the API, because
/// "which variant" without "why" leaves the user guessing whether their
/// override took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason", content = "variant")]
pub enum Resolution {
    /// Pinned explicitly; the frontmost app was not consulted.
    Override(String),
    /// The frontmost app claimed this variant.
    MatchedApp(String),
    /// No variant applied. The layer's own five bindings are live.
    Base,
}

impl Resolution {
    pub fn variant_name(&self) -> Option<&str> {
        match self {
            Resolution::Override(n) | Resolution::MatchedApp(n) => Some(n),
            Resolution::Base => None,
        }
    }
}

/// Choose the live variant.
///
/// Order is deliberate and load-bearing: an override is a person or an agent
/// saying "this one, now", and an app match is a standing rule. A standing
/// rule that could quietly outrank an explicit instruction would make the
/// override untrustworthy -- you would set it, switch windows, and find it
/// gone with nothing said.
///
/// An override naming a variant that does not exist resolves to `Base`
/// rather than to some nearby variant: a typo must not silently arm the
/// wrong bindings.
///
/// When two variants claim the same app the first wins, by declaration
/// order. That is arbitrary but it is *stable*, and the alternative --
/// rejecting the config -- would break a live setup over an ambiguity the
/// user may not care about.
pub fn resolve(variants: &[LayerVariant], ctx: &SwitchContext) -> Resolution {
    if let Some(name) = &ctx.override_name {
        if variants.iter().any(|v| &v.name == name) {
            return Resolution::Override(name.clone());
        }
        return Resolution::Base;
    }
    if let Some(app) = &ctx.frontmost {
        if let Some(v) = variants.iter().find(|v| v.claims(app)) {
            return Resolution::MatchedApp(v.name.clone());
        }
    }
    Resolution::Base
}

/// The action a gesture runs, given the resolved variant.
///
/// Falls back to the layer's own binding whenever the variant does not
/// apply, so a virtual layer is never *less* capable than a fixed one.
pub fn action_for<'a>(
    layer: &'a HostLayer,
    variants: &'a [LayerVariant],
    ctx: &SwitchContext,
    gesture: Gesture,
) -> &'a HostAction {
    match resolve(variants, ctx).variant_name() {
        Some(name) => variants
            .iter()
            .find(|v| v.name == name)
            .map_or_else(|| layer.action(gesture), |v| v.action(gesture)),
        None => layer.action(gesture),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(name: &str, apps: &[&str], twist_l: HostAction) -> LayerVariant {
        LayerVariant {
            name: name.to_string(),
            apps: apps.iter().map(|s| (*s).to_string()).collect(),
            twist_l,
            ..LayerVariant {
                name: String::new(),
                apps: vec![],
                twist_l: HostAction::None,
                twist_r: HostAction::None,
                hold_twist_l: HostAction::None,
                hold_twist_r: HostAction::None,
                press: HostAction::None,
            }
        }
    }

    fn scroll(lines: i32) -> HostAction {
        HostAction::Scroll { lines: Some(lines) }
    }

    fn fixture() -> Vec<LayerVariant> {
        vec![
            variant(
                "Browser",
                &["com.apple.Safari", "com.google.Chrome"],
                scroll(-5),
            ),
            variant("Editor", &["com.microsoft.VSCode"], scroll(-1)),
            variant("Manual", &[], scroll(-9)),
        ]
    }

    fn ctx(frontmost: Option<&str>, over: Option<&str>) -> SwitchContext {
        SwitchContext {
            frontmost: frontmost.map(str::to_string),
            override_name: over.map(str::to_string),
        }
    }

    #[test]
    fn the_frontmost_app_picks_its_variant() {
        let v = fixture();
        assert_eq!(
            resolve(&v, &ctx(Some("com.google.Chrome"), None)),
            Resolution::MatchedApp("Browser".into())
        );
        assert_eq!(
            resolve(&v, &ctx(Some("com.microsoft.VSCode"), None)),
            Resolution::MatchedApp("Editor".into())
        );
    }

    /// An override is someone saying "this one, now". A standing app rule
    /// that could outrank it would make the override untrustworthy.
    #[test]
    fn an_override_outranks_the_frontmost_app() {
        let v = fixture();
        assert_eq!(
            resolve(&v, &ctx(Some("com.google.Chrome"), Some("Editor"))),
            Resolution::Override("Editor".into())
        );
        // A variant no app claims is still reachable by hand.
        assert_eq!(
            resolve(&v, &ctx(Some("com.google.Chrome"), Some("Manual"))),
            Resolution::Override("Manual".into())
        );
    }

    #[test]
    fn an_unclaimed_app_or_no_app_at_all_falls_back_to_the_base() {
        let v = fixture();
        assert_eq!(
            resolve(&v, &ctx(Some("com.apple.Finder"), None)),
            Resolution::Base
        );
        assert_eq!(resolve(&v, &ctx(None, None)), Resolution::Base);
        assert_eq!(
            resolve(&[], &ctx(Some("com.google.Chrome"), None)),
            Resolution::Base
        );
    }

    /// A typo must not arm the nearest variant. Base is the safe answer:
    /// the layer's own bindings, which the user configured deliberately.
    #[test]
    fn an_override_naming_nothing_resolves_to_the_base_not_a_neighbour() {
        let v = fixture();
        assert_eq!(
            resolve(&v, &ctx(Some("com.google.Chrome"), Some("Typo"))),
            Resolution::Base
        );
    }

    /// Switching away and back must return the same answer -- no latching,
    /// no memory of the last match hanging around.
    #[test]
    fn switching_apps_and_back_restores_the_earlier_variant() {
        let v = fixture();
        let browser = resolve(&v, &ctx(Some("com.apple.Safari"), None));
        let _ = resolve(&v, &ctx(Some("com.microsoft.VSCode"), None));
        let elsewhere = resolve(&v, &ctx(Some("com.apple.Finder"), None));
        assert_eq!(elsewhere, Resolution::Base);
        assert_eq!(resolve(&v, &ctx(Some("com.apple.Safari"), None)), browser);
    }

    /// Arbitrary but stable: the alternative is rejecting a live config
    /// over an ambiguity the user may not care about.
    #[test]
    fn when_two_variants_claim_one_app_the_first_declared_wins() {
        let v = vec![
            variant("First", &["com.apple.Safari"], scroll(-1)),
            variant("Second", &["com.apple.Safari"], scroll(-2)),
        ];
        assert_eq!(
            resolve(&v, &ctx(Some("com.apple.Safari"), None)),
            Resolution::MatchedApp("First".into())
        );
    }

    fn base_layer() -> HostLayer {
        HostLayer {
            name: "Virtual".into(),
            twist_l: scroll(-3),
            twist_r: scroll(3),
            hold_twist_l: HostAction::None,
            hold_twist_r: HostAction::None,
            press: HostAction::None,
            variants: vec![],
        }
    }

    #[test]
    fn the_action_comes_from_the_resolved_variant_and_falls_back_to_the_layer() {
        let layer = base_layer();
        let v = fixture();
        assert_eq!(
            action_for(
                &layer,
                &v,
                &ctx(Some("com.apple.Safari"), None),
                Gesture::TwistL
            ),
            &scroll(-5)
        );
        // Unclaimed app: the layer's own binding.
        assert_eq!(
            action_for(
                &layer,
                &v,
                &ctx(Some("com.apple.Finder"), None),
                Gesture::TwistL
            ),
            &scroll(-3)
        );
        // A gesture the variant leaves unset is still the variant's -- an
        // explicit None, not a silent reach back into the base.
        assert_eq!(
            action_for(
                &layer,
                &v,
                &ctx(Some("com.apple.Safari"), None),
                Gesture::TwistR
            ),
            &HostAction::None
        );
    }

    #[test]
    fn a_layer_with_no_variants_behaves_exactly_as_it_did_before() {
        let layer = base_layer();
        for gesture in [
            Gesture::TwistL,
            Gesture::TwistR,
            Gesture::Press,
            Gesture::HoldTwistL,
            Gesture::HoldTwistR,
        ] {
            assert_eq!(
                action_for(&layer, &[], &ctx(Some("com.apple.Safari"), None), gesture),
                layer.action(gesture)
            );
        }
    }
}

/// The user's live config is not a fixture. A schema change that quietly
/// rewrote it -- dropping a field, adding one that serialises where it did
/// not before -- would land as "my settings changed by themselves".
#[cfg(test)]
mod compatibility_tests {
    use crate::host::HostConfig;

    /// Captured from the machine this was developed on, before variants
    /// existed. It must round-trip through the new schema unchanged.
    const LIVE_CONFIG: &str = r#"{
  "layers": [
    {
      "name": "Media",
      "twistL": { "type": "aux", "key": "volumeDown" },
      "twistR": { "type": "aux", "key": "mute" },
      "holdTwistL": { "type": "aux", "key": "brightnessDown" },
      "holdTwistR": { "type": "aux", "key": "brightnessUp" },
      "press": { "type": "aux", "key": "mute" }
    },
    {
      "name": "Navigate",
      "twistL": { "type": "scroll", "lines": -3 },
      "twistR": { "type": "scroll", "lines": 3 },
      "holdTwistL": { "type": "keyChord", "key": 30, "mods": ["cmd"], "label": "]" },
      "holdTwistR": { "type": "keyChord", "key": 24, "mods": ["cmd"], "label": "=" },
      "press": { "type": "keyChord", "key": 15, "mods": ["cmd"], "label": "R" }
    }
  ],
  "doubleTapSwitch": true,
  "doubleTapWindow": 0.25,
  "layerHotkey": null,
  "layerHotkeyBack": null,
  "scrollLinesPerDetent": 3
}"#;

    #[test]
    fn a_config_written_before_variants_existed_round_trips_unchanged() {
        let cfg = HostConfig::try_load_json(LIVE_CONFIG).expect("live config must still parse");
        assert_eq!(cfg.layers.len(), 2);
        assert!(cfg.layers.iter().all(|l| !l.is_virtual()));

        // Re-serialising must not introduce the new field: a fixed layer
        // writes exactly what it wrote before.
        let out = cfg.to_json_pretty();
        assert!(
            !out.contains("variants"),
            "variants leaked into a fixed layer:\n{out}"
        );

        // And the round-trip is stable in both directions.
        let again = HostConfig::try_load_json(&out).expect("re-parse");
        assert_eq!(again.layers, cfg.layers);
    }
}
