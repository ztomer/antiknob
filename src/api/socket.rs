//! Unix domain socket RPC server & client for antiknob-daemon.
//!
//! Serves newline-delimited JSON-RPC requests over a local Unix domain socket.
//! Hardened for DoS-resistance, panic isolation, bounded memory, and secure permissions.

use super::dispatch::{execute_command, ApiContext};
use super::types::Command;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Maximum payload size per JSON-RPC request line (64 KB).
pub const MAX_REQUEST_SIZE: usize = 65_536;

/// Maximum concurrent active socket client connections.
pub const MAX_CONCURRENT_CLIENTS: usize = 16;

/// Active concurrent clients tracking counter.
static ACTIVE_CLIENTS: AtomicUsize = AtomicUsize::new(0);

struct ConnectionGuard;
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CLIENTS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Standard Unix socket path in the user's Application Support directory.
pub fn default_socket_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let dir = PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("antiknob");
    std::fs::create_dir_all(&dir)?;
    // Enforce 0700 permissions on the daemon state directory
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
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

        // Enforce 0600 permissions on the socket file (user-only read/write)
        let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));

        // Create /tmp symlink for easy client discovery
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(&socket_path, TMP_SOCKET_PATH);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let path_clone = socket_path.clone();

        let handle = thread::spawn(move || {
            let _ = listener.set_nonblocking(true);

            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let cur = ACTIVE_CLIENTS.load(Ordering::Relaxed);
                        if cur >= MAX_CONCURRENT_CLIENTS {
                            // Explicit capacity refusal without stalling accept loop
                            let refusal = json!({
                                "jsonrpc": "2.0",
                                "id": Value::Null,
                                "error": {
                                    "code": -32000,
                                    "message": "Server busy: connection capacity reached",
                                    "data": { "retry_after_ms": 50 }
                                }
                            });
                            let mut msg = serde_json::to_string(&refusal).unwrap_or_default();
                            msg.push('\n');
                            let mut s = stream;
                            let _ = s.write_all(msg.as_bytes());
                            let _ = s.flush();
                            continue;
                        }

                        ACTIVE_CLIENTS.fetch_add(1, Ordering::Relaxed);
                        let _guard = ConnectionGuard;
                        handle_client(stream, &mut ctx);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
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

/// Reads a line bounded by max_bytes to prevent unbounded memory allocation DoS.
pub fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    let mut take_reader = reader.take(max_bytes as u64 + 1);
    let n = take_reader.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Request line exceeds {} bytes limit", max_bytes),
        ));
    }
    while buf.ends_with(b"\n") || buf.ends_with(b"\r") {
        buf.pop();
    }
    let s = String::from_utf8(buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(s))
}

fn handle_client(mut stream: UnixStream, ctx: &mut ApiContext) {
    let timeout = Some(Duration::from_secs(5));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);

    let Ok(clone_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone_stream);

    loop {
        match read_bounded_line(&mut reader, MAX_REQUEST_SIZE) {
            Ok(Some(line)) => {
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
            Ok(None) => break,
            Err(e) => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32600, "message": format!("Invalid Request: {}", e) }
                });
                let mut out = serde_json::to_string(&resp).unwrap_or_default();
                out.push('\n');
                let _ = stream.write_all(out.as_bytes());
                let _ = stream.flush();
                break;
            }
        }
    }
}

/// Process a single JSON-RPC line with panic isolation and generate a response.
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

    // Panic isolation: a failure in hardware or command execution must never crash the server
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute_command(ctx, cmd)));

    match result {
        Ok(Ok(val)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": val
        }),
        Ok(Err(e)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": e.to_string() }
        }),
        Err(panic_err) => {
            let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic in dispatcher".to_string()
            };
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32603, "message": format!("Internal error: {}", msg) }
            })
        }
    }
}

/// Send a command to the daemon socket and return the JSON result.
pub fn call_daemon_socket(socket_path: &Path, cmd: &Command) -> Result<Value> {
    let mut last_err = None;
    let mut stream_opt = None;
    for _ in 0..10 {
        match UnixStream::connect(socket_path) {
            Ok(s) => {
                stream_opt = Some(s);
                break;
            }
            Err(e) => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
    let mut stream = match stream_opt {
        Some(s) => s,
        None => {
            return Err(anyhow::anyhow!(
                "Could not connect to socket at {}: {}",
                socket_path.display(),
                last_err.map(|e| e.to_string()).unwrap_or_default()
            ));
        }
    };

    let timeout = Some(Duration::from_secs(30));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);

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
    let response_line = match read_bounded_line(&mut reader, MAX_REQUEST_SIZE)? {
        Some(l) => l,
        None => anyhow::bail!("Daemon closed connection unexpectedly without response"),
    };

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
