//! End-to-end integration tests for the unified API, Unix Domain Socket, and MCP server.

use antiknob::api::{
    all_tools, call_daemon_socket, handle_mcp_request, ApiContext, Command, SocketServer,
};
use antiknob::host::tap::TapEngine;
use antiknob::host::{HostAction, HostConfig, HostLayer};
use serde_json::json;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn temp_api_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("antiknob_e2e_{}_{}", name, std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}

#[test]
fn test_e2e_socket_server_and_client_roundtrip() {
    let dir = temp_api_dir("sock_e2e");
    let sock_path = dir.join("test.sock");
    let config_path = dir.join("host.json");

    let initial_config = HostConfig::default_config();
    fs::write(&config_path, initial_config.to_json_pretty()).unwrap();

    let tap = Arc::new(Mutex::new(TapEngine::new(initial_config)));
    let ctx = ApiContext::new(config_path.clone(), Some(tap.clone()));

    let _server =
        SocketServer::start(sock_path.clone(), ctx).expect("failed to start socket server");
    std::thread::sleep(Duration::from_millis(100));

    // 1. get_status
    let status_res = call_daemon_socket(&sock_path, &Command::GetStatus {}).unwrap();
    assert!(status_res.get("devices").is_some());

    // 2. get_config
    let cfg_res = call_daemon_socket(&sock_path, &Command::GetConfig {}).unwrap();
    assert_eq!(cfg_res["layers"].as_array().unwrap().len(), 2);

    // 3. set_layer
    let layer_res = call_daemon_socket(&sock_path, &Command::SetLayer { layer: 1 }).unwrap();
    assert_eq!(layer_res["active_layer"], 1);
    assert_eq!(tap.lock().unwrap().layer_idx(), 1);

    // 4. set_config
    let mut modified = HostConfig::default_config();
    modified.layers.push(HostLayer {
        name: "TestLayer3".to_string(),
        twist_l: HostAction::None,
        twist_r: HostAction::None,
        hold_twist_l: HostAction::None,
        hold_twist_r: HostAction::None,
        press: HostAction::None,
    });
    let set_cfg_res =
        call_daemon_socket(&sock_path, &Command::SetConfig { config: modified }).unwrap();
    assert_eq!(set_cfg_res["layers_count"], 3);

    // Verify tap engine dynamic hot-reload
    assert_eq!(tap.lock().unwrap().config().layers.len(), 3);

    // 5. list_apps
    let apps_res = call_daemon_socket(&sock_path, &Command::ListApps {}).unwrap();
    assert!(apps_res.get("apps").is_some());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_e2e_mcp_server_protocol_flow() {
    let dir = temp_api_dir("mcp_e2e");
    let config_path = dir.join("host.json");
    let initial_config = HostConfig::default_config();
    fs::write(&config_path, initial_config.to_json_pretty()).unwrap();

    let mut ctx = ApiContext::new(config_path, None);

    // 1. initialize
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    });
    let init_resp = handle_mcp_request(&mut ctx, &init_req.to_string()).unwrap();
    assert_eq!(init_resp["id"], 1);
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "antiknob");

    // 2. ping
    let ping_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping",
        "params": {}
    });
    let ping_resp = handle_mcp_request(&mut ctx, &ping_req.to_string()).unwrap();
    assert_eq!(ping_resp["id"], 2);

    // 3. tools/list
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/list",
        "params": {}
    });
    let list_resp = handle_mcp_request(&mut ctx, &list_req.to_string()).unwrap();
    assert_eq!(list_resp["id"], 3);
    let tools = list_resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), all_tools().len());

    // 4. tools/call get_config
    let call1_req = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "get_config",
            "arguments": {}
        }
    });
    let call1_resp = handle_mcp_request(&mut ctx, &call1_req.to_string()).unwrap();
    assert_eq!(call1_resp["id"], 4);
    let content1 = call1_resp["result"]["content"][0]["text"].as_str().unwrap();
    let parsed_cfg: HostConfig = serde_json::from_str(content1).unwrap();
    assert_eq!(parsed_cfg.layers.len(), 2);

    // 5. tools/call list_apps
    let call2_req = json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "list_apps",
            "arguments": {}
        }
    });
    let call2_resp = handle_mcp_request(&mut ctx, &call2_req.to_string()).unwrap();
    assert_eq!(call2_resp["id"], 5);
    let content2 = call2_resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content2.contains("\"apps\":"));

    let _ = fs::remove_dir_all(dir);
}
