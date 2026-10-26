//! The MCP server's wire behaviour, pinned against the REAL stdio binary.
//!
//! Written before the move onto rmcp (OW.60) so the move is proven
//! behaviour-preserving rather than assumed: every test spawns
//! `antiknob mcp`, writes newline-delimited JSON-RPC to its stdin and reads
//! its stdout, exactly as an MCP host does. Nothing here reaches into the
//! library, so it survives the server being rewritten underneath it.
//!
//! No test calls a tool that writes to hardware: the tools used are the
//! ones that answer without a device (or refuse before touching one).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// A bound on every read: a hung server fails the test, it never hangs it.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Which binary hosts the server: both ship it.
#[derive(Clone, Copy)]
enum Host {
    Cli,
    Daemon,
}

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    dir: PathBuf,
}

impl Session {
    fn start(name: &str) -> Self {
        Self::start_with(name, &[])
    }

    fn start_with(name: &str, extra: &[&str]) -> Self {
        Self::start_on(Host::Cli, name, extra)
    }

    fn start_on(host: Host, name: &str, extra: &[&str]) -> Self {
        let dir =
            std::env::temp_dir().join(format!("antiknob_mcp_stdio_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("host.json");
        std::fs::write(
            &config,
            antiknob::host::HostConfig::default_config().to_json_pretty(),
        )
        .unwrap();
        let (bin, lead): (&str, &[&str]) = match host {
            Host::Cli => (env!("CARGO_BIN_EXE_antiknob"), &["mcp", "--config"]),
            Host::Daemon => (
                env!("CARGO_BIN_EXE_antiknob-daemon"),
                &["--mcp", "--config"],
            ),
        };
        let mut child = Command::new(bin)
            .args(lead)
            .arg(&config)
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn antiknob mcp");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
            dir,
        }
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn send(&mut self, v: &Value) {
        self.send_raw(&v.to_string());
    }

    fn recv(&mut self) -> Value {
        let line = self
            .lines
            .recv_timeout(READ_TIMEOUT)
            .expect("the server answered within the bound");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// Send a request and return its response.
    fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        self.recv()
    }

    fn initialize(&mut self, version: &str) -> Value {
        self.call(
            1,
            "initialize",
            json!({
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": { "name": "stdio-test", "version": "1.0" }
            }),
        )
    }

    fn tool(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.call(
            id,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text_of(resp: &Value) -> &str {
    resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content in {resp}"))
}

#[test]
fn initialize_answers_with_the_server_identity_and_a_tools_capability() {
    let mut s = Session::start("init");
    let r = s.initialize("2025-06-18");
    assert_eq!(r["jsonrpc"], "2.0");
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["serverInfo"]["name"], "antiknob");
    assert_eq!(
        r["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(r["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn initialize_echoes_every_version_that_has_a_handshake() {
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let mut s = Session::start("echo");
        let r = s.initialize(version);
        assert_eq!(r["result"]["protocolVersion"], version, "{r}");
    }
}

#[test]
fn initialize_with_an_unknown_version_answers_a_version_the_server_speaks() {
    let mut s = Session::start("unknown");
    let r = s.initialize("2099-01-01");
    let v = r["result"]["protocolVersion"].as_str().expect("a version");
    assert!(
        ["2025-11-25", "2026-07-28"].contains(&v),
        "an unknown version negotiates to the newest the server speaks, got {v}"
    );
}

#[test]
fn ping_answers_an_empty_result() {
    let mut s = Session::start("ping");
    s.initialize("2025-06-18");
    let r = s.call(2, "ping", json!({}));
    assert_eq!(r["id"], 2);
    assert_eq!(r["result"], json!({}));
}

#[test]
fn a_notification_gets_no_reply() {
    let mut s = Session::start("notify");
    s.initialize("2025-06-18");
    s.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    // If the notification were answered, that answer would arrive first.
    let r = s.call(7, "ping", json!({}));
    assert_eq!(r["id"], 7);
}

fn listed(s: &mut Session) -> Vec<Value> {
    let r = s.call(2, "tools/list", json!({}));
    r["result"]["tools"]
        .as_array()
        .expect("tools array")
        .clone()
}

fn names(tools: &[Value]) -> Vec<String> {
    let mut v: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    v.sort();
    v
}

fn registry_names(destructive: bool) -> Vec<String> {
    use antiknob::api::registry::{self, Effect};
    let mut v: Vec<String> = registry::for_surface(true)
        .filter(|c| destructive || c.effect != Effect::Destructive)
        .map(|c| c.name.to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn tools_list_is_the_registry_with_a_schema_per_tool() {
    let mut s = Session::start_with("list", &["--allow-destructive"]);
    s.initialize("2025-06-18");
    let tools = listed(&mut s);
    assert_eq!(names(&tools), registry_names(true));
    for t in &tools {
        assert!(!t["description"].as_str().unwrap().is_empty(), "{t}");
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
    }
    let bind = tools.iter().find(|t| t["name"] == "bind_sequence").unwrap();
    let required = bind["inputSchema"]["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "key"));
    assert!(required.iter().any(|v| v == "actions"));
}

#[test]
fn destructive_tools_are_not_listed_without_the_opt_in() {
    let mut s = Session::start("list_default");
    s.initialize("2025-06-18");
    let tools = listed(&mut s);
    assert_eq!(names(&tools), registry_names(false));
    assert!(registry_names(true).len() > registry_names(false).len());
}

#[test]
fn a_destructive_tool_is_refused_not_run_without_the_opt_in() {
    let mut s = Session::start("refuse");
    s.initialize("2025-06-18");
    // "zz" is not hex: were the gate broken this fails before any device write.
    let r = s.tool(2, "send_raw", json!({ "bytes": ["zz"] }));
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
    assert!(text_of(&r).contains("--allow-destructive"), "{r}");
}

#[test]
fn every_listed_tool_has_annotations_and_an_object_output_schema_on_the_wire() {
    let mut s = Session::start_with("annotations", &["--allow-destructive"]);
    s.initialize("2025-06-18");
    for t in listed(&mut s) {
        let a = &t["annotations"];
        assert!(a["readOnlyHint"].is_boolean(), "{t}");
        assert!(a["destructiveHint"].is_boolean(), "{t}");
        assert_eq!(t["outputSchema"]["type"], "object", "{t}");
    }
    let tools = listed(&mut s);
    let get = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap().clone();
    assert_eq!(
        get("get_config")["annotations"]["readOnlyHint"],
        json!(true)
    );
    assert_eq!(
        get("upload_keymap")["annotations"]["destructiveHint"],
        json!(true)
    );
    assert_eq!(get("set_led")["annotations"]["readOnlyHint"], json!(false));
    assert_eq!(
        get("set_led")["annotations"]["destructiveHint"],
        json!(false)
    );
}

#[test]
fn a_success_carries_structured_content_equal_to_its_text() {
    let mut s = Session::start("structured");
    s.initialize("2025-06-18");
    let r = s.tool(2, "get_config", json!({}));
    let text: Value = serde_json::from_str(text_of(&r)).unwrap();
    assert_eq!(r["result"]["structuredContent"], text);
}

#[test]
fn the_newest_protocol_is_spoken_per_request_with_no_handshake() {
    let mut s = Session::start("newest");
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": { "name": "stdio-test", "version": "1.0" }
    });
    let d = s.call(1, "server/discover", json!({ "_meta": meta }));
    let versions = d["result"]["supportedVersions"]
        .as_array()
        .expect("versions");
    assert!(versions.iter().any(|v| v == "2026-07-28"), "{d}");
    let r = s.call(2, "tools/list", json!({ "_meta": meta }));
    assert_eq!(
        r["result"]["tools"].as_array().unwrap().len(),
        registry_names(false).len(),
        "{r}"
    );
}

#[test]
fn a_line_that_is_not_json_does_not_end_the_session() {
    let mut s = Session::start("garbage");
    s.initialize("2025-06-18");
    s.send_raw("this is not json");
    let r = s.call(9, "ping", json!({}));
    assert_eq!(r["id"], 9);
}

#[test]
fn get_config_returns_the_host_config_as_pretty_json_text() {
    let mut s = Session::start("cfg");
    s.initialize("2025-06-18");
    let r = s.tool(2, "get_config", json!({}));
    assert_ne!(r["result"]["isError"], json!(true), "{r}");
    assert_eq!(r["result"]["content"][0]["type"], "text");
    let cfg: Value = serde_json::from_str(text_of(&r)).unwrap();
    assert_eq!(cfg["layers"].as_array().unwrap().len(), 2);
}

#[test]
fn a_tool_with_no_device_dependence_answers_text_json() {
    let mut s = Session::start("keys");
    s.initialize("2025-06-18");
    let apps = s.tool(2, "list_apps", json!({}));
    assert!(text_of(&apps).contains("\"apps\":"), "{apps}");
    let ping = s.tool(3, "ping", json!({}));
    assert_eq!(
        serde_json::from_str::<Value>(text_of(&ping)).unwrap(),
        json!({})
    );
    let cmds = s.tool(4, "list_commands", json!({}));
    let parsed: Value = serde_json::from_str(text_of(&cmds)).unwrap();
    assert!(parsed["commands"].as_array().unwrap().len() > 10);
}

#[test]
fn a_command_that_fails_is_an_is_error_result_not_a_protocol_error() {
    let mut s = Session::start("cmderr");
    s.initialize("2025-06-18");
    // Standalone, there is no running tap engine to switch layers on.
    let r = s.tool(2, "set_layer", json!({ "layer": 1 }));
    assert!(r["error"].is_null(), "{r}");
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
    assert!(
        text_of(&r).contains("tap engine not running"),
        "the refusal names its reason: {r}"
    );
}

#[test]
fn bad_arguments_are_an_is_error_result_naming_the_tool() {
    let mut s = Session::start("badargs");
    s.initialize("2025-06-18");
    let r = s.tool(2, "set_layer", json!({}));
    assert!(r["error"].is_null(), "{r}");
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
    assert!(text_of(&r).contains("set_layer"), "{r}");
}

#[test]
fn an_unknown_tool_is_refused_not_run() {
    let mut s = Session::start("unknowntool");
    s.initialize("2025-06-18");
    let r = s.tool(2, "no_such_tool", json!({}));
    let refused = r["error"].is_object() || r["result"]["isError"] == json!(true);
    assert!(refused, "{r}");
}

#[test]
fn an_unknown_method_is_method_not_found() {
    let mut s = Session::start("nomethod");
    s.initialize("2025-06-18");
    let r = s.call(2, "no/such/method", json!({}));
    assert_eq!(r["id"], 2);
    assert_eq!(r["error"]["code"], -32601, "{r}");
}

#[test]
fn the_server_exits_when_stdin_closes() {
    let mut s = Session::start("eof");
    s.initialize("2025-06-18");
    s.stdin.take();
    let deadline = std::time::Instant::now() + READ_TIMEOUT;
    loop {
        if s.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "did not exit at EOF");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_daemon_binary_serves_the_same_tools_and_takes_the_same_opt_in() {
    let mut s = Session::start_on(Host::Daemon, "daemon", &[]);
    let r = s.initialize("2025-06-18");
    assert_eq!(r["result"]["serverInfo"]["name"], "antiknob");
    assert_eq!(names(&listed(&mut s)), registry_names(false));
    let mut s = Session::start_on(Host::Daemon, "daemon_all", &["--allow-destructive"]);
    s.initialize("2025-06-18");
    assert_eq!(names(&listed(&mut s)), registry_names(true));
}

#[test]
fn the_opt_in_means_nothing_without_the_server() {
    // Bounded: were the flag accepted alone this would start a real daemon.
    let mut child = ReapOnDrop(
        Command::new(env!("CARGO_BIN_EXE_antiknob-daemon"))
            .arg("--allow-destructive")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.is_some_and(|s| !s.success()),
        "the flag was accepted alone"
    );
}

/// Kills and reaps its child however the test ends, a failing assertion included.
struct ReapOnDrop(Child);

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
