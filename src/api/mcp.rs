//! Model Context Protocol (MCP) server implementation.
//!
//! Generates MCP tools from the single source of truth (`ALL_TOOLS` in `types.rs`)
//! and dispatches tool calls to `execute_command`.

use super::dispatch::{execute_command, ApiContext};
use super::socket::{read_bounded_line, MAX_REQUEST_SIZE};
use super::types::{all_tools, Command};
use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, Write};

/// Run the MCP server on stdio until EOF.
pub fn run_mcp_server(mut ctx: ApiContext) -> Result<()> {
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let mut stdout = io::stdout();

    loop {
        match read_bounded_line(&mut reader, MAX_REQUEST_SIZE) {
            Ok(Some(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let resp = handle_mcp_request(&mut ctx, trimmed);
                if let Some(resp_val) = resp {
                    let mut out = serde_json::to_string(&resp_val)?;
                    out.push('\n');
                    stdout.write_all(out.as_bytes())?;
                    stdout.flush()?;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("Input error: {}", e) }
                });
                let mut out = serde_json::to_string(&err_resp)?;
                out.push('\n');
                let _ = stdout.write_all(out.as_bytes());
                let _ = stdout.flush();
                break;
            }
        }
    }

    Ok(())
}

/// Handle a single MCP JSON-RPC message. Returns None for notifications.
pub fn handle_mcp_request(ctx: &mut ApiContext, input: &str) -> Option<Value> {
    let req: Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": format!("Parse error: {}", e) }
            }));
        }
    };

    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(json!({}));

    // Notifications have no id
    if id.is_none() && method.starts_with("notifications/") {
        return None;
    }

    let req_id = id.unwrap_or(Value::Null);

    let result = match method {
        "initialize" => {
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "antiknob",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })
        }

        "ping" => json!({}),

        "tools/list" => {
            let tools: Vec<Value> = all_tools()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema
                    })
                })
                .collect();
            json!({ "tools": tools })
        }

        "tools/call" => {
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            let cmd_value = json!({
                "method": tool_name,
                "params": arguments
            });

            match serde_json::from_value::<Command>(cmd_value) {
                Ok(cmd) => {
                    let run_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        execute_command(ctx, cmd)
                    }));
                    match run_res {
                        Ok(Ok(val)) => {
                            let text = serde_json::to_string_pretty(&val).unwrap_or_default();
                            json!({
                                "content": [{
                                    "type": "text",
                                    "text": text
                                }]
                            })
                        }
                        Ok(Err(e)) => json!({
                            "isError": true,
                            "content": [{
                                "type": "text",
                                "text": format!("Error: {}", e)
                            }]
                        }),
                        Err(panic_err) => {
                            let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                                s.to_string()
                            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                                s.clone()
                            } else {
                                "Internal panic during tool execution".to_string()
                            };
                            json!({
                                "isError": true,
                                "content": [{
                                    "type": "text",
                                    "text": format!("Internal error: {}", msg)
                                }]
                            })
                        }
                    }
                }
                Err(e) => json!({
                    "isError": true,
                    "content": [{
                        "type": "text",
                        "text": format!("Invalid arguments for tool '{}': {}", tool_name, e)
                    }]
                }),
            }
        }

        other => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "error": { "code": -32601, "message": format!("Method '{}' not found", other) }
            }));
        }
    };

    Some(json!({
        "jsonrpc": "2.0",
        "id": req_id,
        "result": result
    }))
}
