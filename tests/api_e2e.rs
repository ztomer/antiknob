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
    assert!(status_res.get("transport").is_some());
    assert!(status_res["power"].get("transport").is_some());
    assert!(status_res.get("tap_active").is_some());
    assert_eq!(status_res["tap_active"].as_bool(), Some(false));
    assert!(status_res.get("tap_error").is_some());

    // 2. get_config
    let cfg_res = call_daemon_socket(&sock_path, &Command::GetConfig {}).unwrap();
    assert_eq!(cfg_res["layers"].as_array().unwrap().len(), 2);

    // 3. set_layer
    let layer_res = call_daemon_socket(&sock_path, &Command::SetLayer { layer: 1 }).unwrap();
    assert_eq!(layer_res["active_layer"], 1);
    assert_eq!(tap.lock().unwrap().layer_idx(), 1);

    // 4. set_config
    let mut c = tap.lock().unwrap().config().clone();
    c.bound_device_layers = vec![0, 1, 2];
    tap.lock().unwrap().apply_config(c);
    let mut modified = HostConfig::default_config();
    modified.bound_device_layers = Vec::new(); // Simulate UI client omitting bound layers
    modified.layers.push(HostLayer {
        name: "TestLayer3".to_string(),
        twist_l: HostAction::None,
        twist_r: HostAction::None,
        hold_twist_l: HostAction::None,
        hold_twist_r: HostAction::None,
        press: HostAction::None,
        variants: vec![],
        led: None,
    });
    let set_cfg_res =
        call_daemon_socket(&sock_path, &Command::SetConfig { config: modified }).unwrap();
    assert_eq!(set_cfg_res["layers_count"], 3);

    // Verify tap engine dynamic hot-reload and bound layers preservation
    assert_eq!(tap.lock().unwrap().config().layers.len(), 3);
    assert_eq!(
        tap.lock().unwrap().config().bound_device_layers,
        vec![0, 1, 2]
    );

    let cfg_res2 = call_daemon_socket(&sock_path, &Command::GetConfig {}).unwrap();
    assert_eq!(cfg_res2["boundDeviceLayers"], json!([0, 1, 2]));

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

#[test]
fn test_socket_permissions_and_ping() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_api_dir("sock_perm");
    let sock_path = dir.join("test.sock");
    let config_path = dir.join("host.json");

    let initial_config = HostConfig::default_config();
    fs::write(&config_path, initial_config.to_json_pretty()).unwrap();

    let ctx = ApiContext::new(config_path, None);
    let _server =
        SocketServer::start(sock_path.clone(), ctx).expect("failed to start socket server");
    std::thread::sleep(Duration::from_millis(100));

    // Verify 0600 socket permissions
    let meta = fs::metadata(&sock_path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "Socket must have 0600 permissions");

    // Verify ping command
    let ping_res = call_daemon_socket(&sock_path, &Command::Ping {}).unwrap();
    assert!(ping_res.is_object());

    // Verify power status in GetStatus
    let status_res = call_daemon_socket(&sock_path, &Command::GetStatus {}).unwrap();
    assert!(status_res.get("power").is_some());
    let desc = status_res["power"]["description"].as_str().unwrap();
    assert!(desc.contains("Powered") || desc.contains("Disconnected"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_socket_dos_bounded_read_protection() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let dir = temp_api_dir("sock_dos");
    let sock_path = dir.join("test.sock");
    let config_path = dir.join("host.json");

    let initial_config = HostConfig::default_config();
    fs::write(&config_path, initial_config.to_json_pretty()).unwrap();

    let ctx = ApiContext::new(config_path, None);
    let _server =
        SocketServer::start(sock_path.clone(), ctx).expect("failed to start socket server");
    std::thread::sleep(Duration::from_millis(100));

    // 1. Send >64KB of characters without newline
    let mut stream = UnixStream::connect(&sock_path).unwrap();
    let oversized = vec![b'x'; 70_000];
    let _ = stream.write_all(&oversized);
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();

    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    let _ = reader.read_line(&mut resp);

    // Bounded line reader must reject payload with -32600 error
    assert!(resp.contains("-32600") || resp.is_empty());

    // 2. Server must remain healthy and accept subsequent valid requests
    let ping_res = call_daemon_socket(&sock_path, &Command::Ping {}).unwrap();
    assert!(ping_res.is_object());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_json_rpc_error_handling() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let dir = temp_api_dir("sock_err");
    let sock_path = dir.join("test.sock");
    let config_path = dir.join("host.json");

    let initial_config = HostConfig::default_config();
    fs::write(&config_path, initial_config.to_json_pretty()).unwrap();

    let ctx = ApiContext::new(config_path, None);
    let _server =
        SocketServer::start(sock_path.clone(), ctx).expect("failed to start socket server");
    std::thread::sleep(Duration::from_millis(100));

    // 1. Malformed JSON returns -32700
    let mut stream = UnixStream::connect(&sock_path).unwrap();
    stream.write_all(b"not a valid json request\n").unwrap();
    stream.flush().unwrap();

    let mut reader = BufReader::new(&stream);
    let mut resp = String::new();
    reader.read_line(&mut resp).unwrap();
    assert!(resp.contains("-32700"));
    drop(stream);

    // 2. Unknown method returns -32601
    let mut stream2 = UnixStream::connect(&sock_path).unwrap();
    stream2
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"nonexistent_tool\"}\n")
        .unwrap();
    stream2.flush().unwrap();

    let mut reader2 = BufReader::new(&stream2);
    let mut resp2 = String::new();
    reader2.read_line(&mut resp2).unwrap();
    assert!(resp2.contains("-32601"));
    drop(stream2);

    // 3. Oversized SendRaw is rejected by input validation
    let oversized_raw = Command::SendRaw {
        bytes: (0..70).map(|i| format!("{:02x}", i)).collect(),
    };
    let err = call_daemon_socket(&sock_path, &oversized_raw).unwrap_err();
    assert!(err.to_string().contains("exceeds 64-byte limit"));

    let _ = fs::remove_dir_all(dir);
}
