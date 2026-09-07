//! Unix domain socket RPC server & client for antiknob-daemon.
//!
//! Serves newline-delimited JSON-RPC requests over a local Unix domain socket.

use super::dispatch::{execute_command, ApiContext};
use super::types::Command;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Standard Unix socket path in the user's Application Support directory.
pub fn default_socket_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let dir = PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("antiknob");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("antiknob.sock"))
}

/// Fallback / convenience symlink location in /tmp.
pub const TMP_SOCKET_PATH: &str = "/tmp/antiknob.sock";

/// Spawns the Unix domain socket server on a background thread.
pub struct SocketServer {
    socket_path: PathBuf,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SocketServer {
    pub fn start(socket_path: PathBuf, mut ctx: ApiContext) -> Result<Self> {
        // Clean up any stale socket files from prior runs
        let _ = std::fs::remove_file(&socket_path);
        let _ = std::fs::remove_file(TMP_SOCKET_PATH);

        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("Failed to bind Unix socket at {}", socket_path.display()))?;

        // Create /tmp symlink for easy client discovery
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(&socket_path, TMP_SOCKET_PATH);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let path_clone = socket_path.clone();

        let handle = thread::spawn(move || {
            // Set listener non-blocking with small sleep so we can check running flag
            let _ = listener.set_nonblocking(true);

            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        handle_client(stream, &mut ctx);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            let _ = std::fs::remove_file(&path_clone);
            let _ = std::fs::remove_file(TMP_SOCKET_PATH);
        });

        Ok(Self {
            socket_path,
            running,
            handle: Some(handle),
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for SocketServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(TMP_SOCKET_PATH);
    }
}

fn handle_client(mut stream: UnixStream, ctx: &mut ApiContext) {
    let reader = BufReader::new(stream.try_clone().unwrap());
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let resp = process_json_rpc(ctx, trimmed);
        let mut out = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
        out.push('\n');
        if stream.write_all(out.as_bytes()).is_err() {
            break;
        }
        let _ = stream.flush();
    }
}

/// Process a single JSON-RPC line and generate a response.
pub fn process_json_rpc(ctx: &mut ApiContext, input: &str) -> Value {
    let req: Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            return json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": format!("Parse error: {}", e) }
            });
        }
    };

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(json!({}));

    // Reconstruct Command enum from method and params
    let cmd_value = json!({
        "method": method,
        "params": params
    });

    let cmd: Command = match serde_json::from_value(cmd_value) {
        Ok(c) => c,
        Err(e) => {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Unknown method or bad params for '{}': {}", method, e) }
            });
        }
    };

    match execute_command(ctx, cmd) {
        Ok(val) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": val
        }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": e.to_string() }
        }),
    }
}

/// Send a command to the daemon socket and return the JSON result.
pub fn call_daemon_socket(socket_path: &Path, cmd: &Command) -> Result<Value> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("Could not connect to socket at {}", socket_path.display()))?;

    let cmd_val = serde_json::to_value(cmd)?;
    let method = cmd_val.get("method").and_then(Value::as_str).unwrap_or("");
    let params = cmd_val.get("params").cloned().unwrap_or(json!({}));

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });

    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let resp: Value = serde_json::from_str(&response_line)?;
    if let Some(err) = resp.get("error") {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("RPC error");
        anyhow::bail!("{}", msg);
    }

    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
}
