//! Unified API module providing single source of truth for RPC and MCP.

pub mod dispatch;
pub mod mcp;
pub mod socket;
pub mod types;

pub use dispatch::{execute_command, ApiContext, TapHealth};
pub use mcp::{handle_mcp_request, run_mcp_server};
pub use socket::{call_daemon_socket, default_socket_path, SocketServer, TMP_SOCKET_PATH};
pub use types::{all_tools, Command, ToolDef};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::HostConfig;
    use serde_json::json;

    #[test]
    fn all_tools_have_unique_names_and_valid_schemas() {
        let mut names = std::collections::HashSet::new();
        let tools = all_tools();
        for tool in &tools {
            assert!(
                names.insert(tool.name),
                "Duplicate tool name: {}",
                tool.name
            );
            assert!(!tool.description.is_empty());
            assert_eq!(tool.input_schema["type"], "object");
        }
    }

    #[test]
    fn command_serialization_matches_tool_names() {
        let cmd = Command::SetLed {
            layer: 1,
            mode: "backlight".to_string(),
            color: Some("cyan".to_string()),
        };
        let val = serde_json::to_value(&cmd).unwrap();
        assert_eq!(val["method"], "set_led");
        assert_eq!(val["params"]["layer"], 1);
        assert_eq!(val["params"]["mode"], "backlight");
        assert_eq!(val["params"]["color"], "cyan");
    }

    #[test]
    fn mcp_initialize_and_tools_list() {
        let temp_dir =
            std::env::temp_dir().join(format!("antiknob_test_mcp_{}", std::process::id()));
        let mut ctx = ApiContext::new(temp_dir.join("host.json"), None);

        // Initialize
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let init_resp = handle_mcp_request(&mut ctx, &init_req.to_string()).unwrap();
        assert_eq!(init_resp["id"], 1);
        assert!(init_resp["result"]["capabilities"]["tools"].is_object());

        // Tools list
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let list_resp = handle_mcp_request(&mut ctx, &list_req.to_string()).unwrap();
        assert_eq!(list_resp["id"], 2);
        let tools = list_resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), all_tools().len());

        // Call tool: get_status
        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "get_status",
                "arguments": {}
            }
        });
        let call_resp = handle_mcp_request(&mut ctx, &call_req.to_string()).unwrap();
        assert_eq!(call_resp["id"], 3);
        assert_eq!(call_resp["result"]["content"][0]["type"], "text");
    }

    #[test]
    fn socket_server_client_roundtrip() {
        let temp_dir =
            std::env::temp_dir().join(format!("antiknob_test_sock_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let sock_path = temp_dir.join("test.sock");
        let cfg_path = temp_dir.join("host.json");

        let ctx = ApiContext::new(cfg_path, None);
        let server = SocketServer::start(sock_path.clone(), ctx).unwrap();

        let resp = call_daemon_socket(&sock_path, &Command::GetStatus {}).unwrap();
        assert!(resp["connected"].is_boolean());

        // Test GetConfig over socket
        let cfg_val = call_daemon_socket(&sock_path, &Command::GetConfig {}).unwrap();
        assert!(cfg_val["layers"].is_array());

        // Test SetConfig over socket
        let mut cfg = HostConfig::default_config();
        cfg.double_tap_switch = false;
        let set_val = call_daemon_socket(&sock_path, &Command::SetConfig { config: cfg }).unwrap();
        assert_eq!(set_val["ok"], true);

        // Verify updated
        let updated_cfg = call_daemon_socket(&sock_path, &Command::GetConfig {}).unwrap();
        assert_eq!(updated_cfg["doubleTapSwitch"], false);

        drop(server);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
