//! Engine-level tests for the virtual layer.
//!
//! Split from `engine.rs` for the file-length gate. They drive real slot
//! chords through `handle_chord`, so they exercise the production path
//! rather than reaching into the resolver directly.

#[cfg(test)]
mod tests {
    use crate::host::engine::{Engine, EngineEvent, FiredAction};
    use crate::host::frontmost::FakeApp;
    use crate::host::virtual_layer::{LayerVariant, Resolution};
    use crate::host::{slot_chord, Gesture, HostAction, HostConfig, HostLayer};
    use std::sync::Arc;

    fn scroll(lines: i32) -> HostAction {
        HostAction::Scroll { lines: Some(lines) }
    }

    fn variant(name: &str, apps: &[&str], lines: i32) -> LayerVariant {
        LayerVariant {
            name: name.into(),
            apps: apps.iter().map(|s| (*s).to_string()).collect(),
            twist_l: scroll(lines),
            twist_r: HostAction::None,
            hold_twist_l: HostAction::None,
            hold_twist_r: HostAction::None,
            press: HostAction::None,
        }
    }

    fn cfg_with_virtual_layer() -> HostConfig {
        HostConfig {
            layers: vec![HostLayer {
                name: "Virtual".into(),
                twist_l: scroll(-3),
                twist_r: HostAction::None,
                hold_twist_l: HostAction::None,
                hold_twist_r: HostAction::None,
                press: HostAction::None,
                variants: vec![
                    variant("Browser", &["com.apple.Safari"], -5),
                    variant("Editor", &["com.microsoft.VSCode"], -1),
                ],
            }],
            ..HostConfig::default_config()
        }
    }

    /// Drive a real TwistL slot chord through the engine and report what
    /// fired -- the production path, not a shortcut into `action_for`.
    fn twist_l(engine: &mut Engine) -> Option<i32> {
        let (key, mods) = slot_chord(Gesture::TwistL);
        engine
            .handle_chord(key, &mods, 0)
            .into_iter()
            .find_map(|e| match e {
                EngineEvent::Fire(FiredAction::Scroll { lines }) => Some(lines),
                _ => None,
            })
    }

    #[test]
    fn the_frontmost_app_decides_which_variant_fires() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut engine = Engine::with_frontmost(cfg_with_virtual_layer(), app.clone());
        assert_eq!(twist_l(&mut engine), Some(-5), "Safari must pick Browser");

        app.switch_to(Some("com.microsoft.VSCode"));
        assert_eq!(twist_l(&mut engine), Some(-1), "VSCode must pick Editor");

        // An app no variant claims falls back to the layer's own binding.
        app.switch_to(Some("com.apple.Finder"));
        assert_eq!(twist_l(&mut engine), Some(-3), "Finder must fall back");
    }

    /// The read happens when the gesture fires, not when the engine was
    /// built -- otherwise the knob would obey whatever was in front at login.
    #[test]
    fn the_app_is_read_at_fire_time_not_at_construction() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Finder")));
        let mut engine = Engine::with_frontmost(cfg_with_virtual_layer(), app.clone());
        assert_eq!(twist_l(&mut engine), Some(-3));
        app.switch_to(Some("com.apple.Safari"));
        assert_eq!(
            twist_l(&mut engine),
            Some(-5),
            "a cached app would still be firing the fallback"
        );
    }

    #[test]
    fn an_override_pins_a_variant_and_clearing_it_releases() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut engine = Engine::with_frontmost(cfg_with_virtual_layer(), app);
        engine.set_variant_override(Some("Editor".into()));
        assert_eq!(
            twist_l(&mut engine),
            Some(-1),
            "the pin must outrank Safari"
        );
        assert_eq!(engine.resolution(), Resolution::Override("Editor".into()));

        engine.set_variant_override(None);
        assert_eq!(
            twist_l(&mut engine),
            Some(-5),
            "clearing must release the pin"
        );
        assert_eq!(
            engine.resolution(),
            Resolution::MatchedApp("Browser".into())
        );
    }

    /// A layer with no variants must behave exactly as it did before this
    /// existed, whatever is in front.
    #[test]
    fn a_fixed_layer_ignores_the_frontmost_app_entirely() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut cfg = cfg_with_virtual_layer();
        cfg.layers[0].variants.clear();
        let mut engine = Engine::with_frontmost(cfg, app.clone());
        assert_eq!(twist_l(&mut engine), Some(-3));
        app.switch_to(Some("com.microsoft.VSCode"));
        assert_eq!(twist_l(&mut engine), Some(-3));
        assert_eq!(engine.resolution(), Resolution::Base);
    }
}
