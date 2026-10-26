//! The server's decisions, without a runtime or a pipe.

use super::*;
use crate::api::registry;
use crate::host::HostConfig;

fn server(name: &str, allow_destructive: bool) -> (AntiknobMcp, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("antiknob_mcp_unit_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("host.json");
    std::fs::write(&config, HostConfig::default_config().to_json_pretty()).unwrap();
    let ctx = ApiContext::new(config, None);
    (AntiknobMcp::new(ctx, McpOptions { allow_destructive }), dir)
}

fn args(v: &Value) -> Option<JsonObject> {
    v.as_object().cloned()
}

fn text(result: &CallToolResult) -> String {
    result.content[0]
        .as_text()
        .expect("text content")
        .text
        .clone()
}

#[test]
fn a_destructive_tool_is_not_listed_without_the_opt_in() {
    let (s, dir) = server("list_default", false);
    let names: Vec<String> = s.tools().iter().map(|t| t.name.to_string()).collect();
    for spec in registry::for_surface(true) {
        let listed = names.iter().any(|n| n == spec.name);
        assert_eq!(listed, spec.effect != Effect::Destructive, "{}", spec.name);
    }
    // The class is not empty: otherwise the opt-in guards nothing.
    assert!(registry::for_surface(true).any(|s| s.effect == Effect::Destructive));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_opt_in_lists_every_tool_the_registry_exposes() {
    let (s, dir) = server("list_all", true);
    let names: Vec<String> = s.tools().iter().map(|t| t.name.to_string()).collect();
    let want: Vec<&str> = registry::for_surface(true).map(|c| c.name).collect();
    assert_eq!(names, want);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_tool_carries_annotations_that_match_its_effect_and_an_object_output_schema() {
    let (s, dir) = server("annotations", true);
    for tool in s.tools() {
        let spec = registry::COMMANDS
            .iter()
            .find(|c| c.name == tool.name)
            .unwrap();
        let a = tool.annotations.as_ref().expect("annotations");
        assert_eq!(
            a.read_only_hint,
            Some(spec.effect == Effect::Read),
            "{}",
            tool.name
        );
        assert_eq!(
            a.destructive_hint,
            Some(spec.effect == Effect::Destructive),
            "{}",
            tool.name
        );
        assert_eq!(a.open_world_hint, Some(false));
        let out = tool.output_schema.as_ref().expect("outputSchema");
        assert_eq!(out.get("type"), Some(&json!("object")), "{}", tool.name);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_destructive_tool_is_refused_not_run_without_the_opt_in() {
    let (s, dir) = server("refused", false);
    // "zz" is not hex: were the gate broken this fails before any device write.
    let r = s.call("send_raw", args(&json!({ "bytes": ["zz"] })));
    assert_eq!(r.is_error, Some(true));
    assert!(text(&r).contains("--allow-destructive"), "{}", text(&r));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_success_is_pretty_text_plus_the_same_object_as_structured_content() {
    let (s, dir) = server("success", false);
    let r = s.call("get_config", None);
    assert_eq!(r.is_error, Some(false));
    let structured = r.structured_content.clone().expect("structuredContent");
    assert_eq!(structured["layers"].as_array().unwrap().len(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(&text(&r)).unwrap(),
        structured
    );
    assert!(text(&r).contains('\n'), "the text stays pretty-printed");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn replies_with_a_fixed_shape_satisfy_their_declared_output_schema() {
    let (s, dir) = server("shape", false);
    for name in [
        "get_config",
        "ping",
        "list_apps",
        "show_keys",
        "list_commands",
    ] {
        let r = s.call(name, None);
        let structured = r.structured_content.expect(name);
        let spec = registry::COMMANDS.iter().find(|c| c.name == name).unwrap();
        for (field, ty) in spec.returns {
            let v = &structured[*field];
            let ok = match *ty {
                "array" => v.is_array(),
                "object" => v.is_object(),
                "string" => v.is_string(),
                "boolean" => v.is_boolean(),
                "integer" => v.is_i64() || v.is_u64(),
                other => panic!("unknown schema type {other}"),
            };
            assert!(ok, "{name}.{field} is not {ty}: {v}");
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_reply_that_is_not_an_object_is_wrapped_to_meet_the_schema() {
    let r = success(json!([1, 2]));
    assert_eq!(r.structured_content, Some(json!({ "result": [1, 2] })));
}

#[test]
fn a_command_that_fails_is_an_is_error_result() {
    let (s, dir) = server("fails", false);
    let r = s.call("set_layer", args(&json!({ "layer": 1 })));
    assert_eq!(r.is_error, Some(true));
    assert!(text(&r).starts_with("Error: "), "{}", text(&r));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bad_or_unknown_calls_name_the_tool() {
    let (s, dir) = server("invalid", false);
    for name in ["set_layer", "no_such_tool"] {
        let r = s.call(name, None);
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains(&format!("Invalid arguments for tool '{name}'")));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn set_layer_is_called_by_the_name_its_schema_advertises() {
    // The schema once said `index` while the command read `layer`, so a
    // client that followed the schema was refused every time.
    let (s, dir) = server("set_layer_schema", false);
    let tools = s.tools();
    let tool = tools.iter().find(|t| t.name == "set_layer").unwrap();
    let props = tool
        .input_schema
        .get("properties")
        .unwrap()
        .as_object()
        .unwrap();
    for field in props.keys() {
        let r = s.call("set_layer", args(&json!({ field: 0 })));
        assert!(
            !text(&r).contains("Invalid arguments"),
            "the advertised field '{field}' is not the one the command reads: {}",
            text(&r)
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn set_config_advertises_the_object_the_command_takes() {
    let (s, dir) = server("set_config_schema", true);
    let tools = s.tools();
    let tool = tools.iter().find(|t| t.name == "set_config").unwrap();
    assert_eq!(tool.input_schema["properties"]["config"]["type"], "object");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_panic_is_reported_not_propagated() {
    assert_eq!(panic_message(&"boom"), "boom");
    assert_eq!(panic_message(&String::from("bang")), "bang");
    assert!(panic_message(&7_u8).contains("Internal panic"));
}

#[test]
fn the_server_announces_itself_and_a_tools_capability() {
    let (s, dir) = server("info", false);
    let info = s.get_info();
    assert_eq!(info.server_info.name, "antiknob");
    assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
    assert!(info.capabilities.tools.is_some());
    let _ = std::fs::remove_dir_all(dir);
}
