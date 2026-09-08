//! Socket/MCP tests for the virtual layer commands.
//!
//! Split from `dispatch.rs` for the file-length gate. They go through
//! `execute_command`, so the wire shape the daemon actually answers with is
//! what is asserted -- including the resolution *reason*, which is the part
//! a caller needs to tell a pin from a coincidence.

#[cfg(test)]
mod tests {
    use crate::api::dispatch::{execute_command, ApiContext};
    use crate::api::types::Command;
    use crate::host::frontmost::FakeApp;
    use crate::host::tap::TapEngine;
    use crate::host::virtual_layer::LayerVariant;
    use crate::host::HostConfig;
    use crate::host::{HostAction, HostLayer};
    use serde_json::Value;
    use std::sync::Arc;
    use std::sync::Mutex;

    fn variant(name: &str, apps: &[&str]) -> LayerVariant {
        LayerVariant {
            name: name.into(),
            apps: apps.iter().map(|s| (*s).to_string()).collect(),
            twist_l: HostAction::None,
            twist_r: HostAction::None,
            hold_twist_l: HostAction::None,
            hold_twist_r: HostAction::None,
            press: HostAction::None,
        }
    }

    fn ctx_with_virtual_layer(app: Arc<FakeApp>) -> ApiContext {
        ctx_bound_to(app, None)
    }

    fn ctx_bound_to(app: Arc<FakeApp>, bound: Option<u8>) -> ApiContext {
        let cfg = HostConfig {
            bound_device_layer: bound,
            layers: vec![HostLayer {
                name: "Virtual".into(),
                twist_l: HostAction::None,
                twist_r: HostAction::None,
                hold_twist_l: HostAction::None,
                hold_twist_r: HostAction::None,
                press: HostAction::None,
                variants: vec![
                    variant("Browser", &["com.apple.Safari"]),
                    variant("Editor", &["com.microsoft.VSCode"]),
                ],
                led: None,
            }],
            ..HostConfig::default_config()
        };
        let engine = TapEngine::with_frontmost(cfg, app);
        ApiContext::new(
            std::env::temp_dir().join(format!("antiknob-vl-{}.json", std::process::id())),
            Some(Arc::new(Mutex::new(engine))),
        )
    }

    fn get(ctx: &mut ApiContext) -> Value {
        execute_command(ctx, Command::GetVirtualLayer {}).expect("get_virtual_layer")
    }

    #[test]
    fn the_report_names_the_variant_and_why_it_is_live() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut ctx = ctx_with_virtual_layer(app.clone());

        let v = get(&mut ctx);
        assert_eq!(v["is_virtual"], true);
        assert_eq!(v["active_variant"], "Browser");
        assert_eq!(v["resolution"]["reason"], "matched_app");

        app.switch_to(Some("com.apple.Finder"));
        let v = get(&mut ctx);
        assert!(v["active_variant"].is_null(), "no variant claims Finder");
        assert_eq!(v["resolution"]["reason"], "base");
    }

    #[test]
    fn a_pin_outranks_the_app_and_clearing_it_releases() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut ctx = ctx_with_virtual_layer(app);

        let set = execute_command(
            &mut ctx,
            Command::SetVirtualVariant {
                variant: Some("Editor".into()),
            },
        )
        .expect("pin");
        assert_eq!(set["active_variant"], "Editor");
        assert_eq!(set["resolution"]["reason"], "override");

        // Clearing must actually release, not latch the last pin.
        let cleared =
            execute_command(&mut ctx, Command::SetVirtualVariant { variant: None }).expect("clear");
        assert!(cleared["override"].is_null());
        assert_eq!(cleared["active_variant"], "Browser");
        assert_eq!(cleared["resolution"]["reason"], "matched_app");
    }

    /// Storing an unknown name would report "ok" for a pin that never took:
    /// the resolver falls back to the base and the caller is none the wiser.
    #[test]
    fn pinning_a_variant_that_does_not_exist_is_refused_and_names_the_real_ones() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut ctx = ctx_with_virtual_layer(app);
        let err = execute_command(
            &mut ctx,
            Command::SetVirtualVariant {
                variant: Some("Nope".into()),
            },
        )
        .expect_err("an unknown variant must be refused");
        let msg = err.to_string();
        assert!(msg.contains("Nope"), "{msg}");
        assert!(
            msg.contains("Browser"),
            "the error must name what does exist: {msg}"
        );

        // And nothing was stored.
        assert!(get(&mut ctx)["override"].is_null());
    }

    #[test]
    fn both_commands_are_in_the_tool_list() {
        let names: Vec<&str> = crate::api::all_tools().iter().map(|t| t.name).collect();
        assert!(names.contains(&"get_virtual_layer"), "{names:?}");
        assert!(names.contains(&"set_virtual_variant"), "{names:?}");
    }

    /// A virtual layer on a device nothing was bound to is configured
    /// perfectly and fires never. Before the report carried the
    /// arrangement, the only symptom was a knob that did nothing, with no
    /// error and nothing to search for.
    #[test]
    fn the_report_says_when_the_daemon_cannot_hear_the_knob_at_all() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut ctx = ctx_bound_to(app, None);

        let v = get(&mut ctx);
        assert_eq!(v["can_fire"], false);
        assert_eq!(v["device_binding"]["state"], "unbound");
        let said = v["device_binding_summary"].as_str().expect("summary");
        assert!(said.contains("never hears"), "{said}");
        assert!(said.contains("no host layer can fire"), "{said}");
        // Not "bind-slots", which this used to assert. The summary is read
        // by a terminal and by the settings app, so it states the fact and
        // leaves the next step to whichever surface is rendering it; see
        // `host::device_binding::no_description_tells_the_reader_to_run_a_command`.
        assert!(!said.contains("bind-slots"), "{said}");
        // The variant still resolves -- the config is fine. It is the
        // hardware arrangement that is not, and the two must not be
        // conflated in the report.
        assert_eq!(v["active_variant"], "Browser");
    }

    /// With a layer bound, the report names it AND names the ones the
    /// daemon is not involved in, so a user can see that two of their three
    /// layers work with the daemon stopped.
    #[test]
    fn the_report_names_the_bound_layer_and_the_standalone_ones() {
        let app = Arc::new(FakeApp::new(Some("com.apple.Safari")));
        let mut ctx = ctx_bound_to(app, Some(1));

        let v = get(&mut ctx);
        assert_eq!(v["can_fire"], true);
        assert_eq!(v["device_binding"]["state"], "bound");
        assert_eq!(v["device_binding"]["host_translated"], 1);
        let said = v["device_binding_summary"].as_str().expect("summary");
        assert!(said.contains("Device layer 2 is host-translated"), "{said}");
        assert!(said.contains("standalone"), "{said}");
    }
}
