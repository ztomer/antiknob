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
        let cfg = HostConfig {
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
}
