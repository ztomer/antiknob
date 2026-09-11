//! Runtime policy: every process budget in one place.
//!
//! The companion to `firmware.rs`. That module holds what the HARDWARE is;
//! this one holds what the PROCESSES allow themselves: socket capacities
//! and timeouts, request-validation caps, retry budgets, backup rotation,
//! and the login-item label. Same rule, other direction: a budget that
//! describes process behaviour goes here, with a doc line naming the loop
//! it paces. User configuration (`host.json`, CLI flags) and test
//! scaffolding stay where they are -- those are inputs and fixtures, not
//! policy.
//!
//! Durations are plain numbers in the unit the call site needs (`_MS`,
//! `_SECS`), because `Duration` constants cannot feed the `i32`
//! milliseconds `hidapi` timeouts take without a cast at every site, and
//! a cast at every site is how a millisecond becomes a second.

// ---------------------------------------------------------------------------
// Socket server: capacities, timeouts, and paths.
// ---------------------------------------------------------------------------

/// Largest JSON-RPC request line accepted, bounding memory per connection
/// against an unbounded-read DoS.
pub const MAX_REQUEST_SIZE: usize = 65_536;

/// Concurrent client connections before the server refuses with
/// "Server busy" instead of stalling its accept loop.
pub const MAX_CONCURRENT_CLIENTS: usize = 16;

/// Convenience symlink to the real socket, for client discovery.
pub const TMP_SOCKET_PATH: &str = "/tmp/antiknob.sock";

/// File mode of the bound socket (user-only read/write).
pub const SOCKET_FILE_MODE: u32 = 0o600;

/// Directory mode of the daemon state dir under Application Support.
pub const STATE_DIR_MODE: u32 = 0o700;

/// Read/write timeout on an accepted client connection, in seconds.
pub const SOCKET_IO_TIMEOUT_SECS: u64 = 5;

/// Read/write timeout for one-shot client calls (`call_daemon_socket`),
/// in seconds. Longer than the server side: a full slot-table read walks
/// the device while the client waits.
pub const SOCKET_CALL_TIMEOUT_SECS: u64 = 30;

/// Poll interval of the socket accept loop and the daemon event loops, in
/// milliseconds. One value for all three: they all drain a queue, none of
/// them waits anything out.
pub const LOOP_TICK_MS: u64 = 50;

/// Connect retries for one-shot client calls: the daemon may be mid-start
/// when the first attempt lands. This many tries, this far apart, then the
/// "could not connect" error below.
pub const SOCKET_CONNECT_RETRIES: usize = 10;
pub const SOCKET_CONNECT_RETRY_MS: u64 = 25;

// ---------------------------------------------------------------------------
// Request validation caps.
// ---------------------------------------------------------------------------

/// Largest standalone keymap accepted for flashing, in bytes.
pub const KEYMAP_YAML_MAX_BYTES: usize = 65_536;

/// Longest `set_led` mode string, in characters.
pub const SET_LED_SPEC_MAX_LEN: usize = 64;

/// Highest device layer `set_led` addresses. Layer indexes above this are
/// refused rather than written somewhere unmapped.
pub const SET_LED_LAYER_MAX: u8 = 15;

/// Longest raw HID payload `send_raw` accepts, in bytes: one report.
pub const RAW_PAYLOAD_MAX_LEN: usize = 64;

/// Most slot counters one `read_slots` call walks.
pub const READ_SLOTS_COUNTERS_MAX: usize = 16;

// ---------------------------------------------------------------------------
// Daemon supervisor and login item.
// ---------------------------------------------------------------------------

/// Delay before rebuilding a dead event tap, in seconds. The supervisor
/// rebuilds forever; the reason is only re-logged when it changes.
pub const TAP_REBUILD_DELAY_SECS: u64 = 3;

/// `bootout` returns before the job is gone, so uninstall polls for the
/// label to clear: this many tries, this far apart, then gives up loudly
/// rather than bootstrapping into a domain that still holds it.
pub const BOOTOUT_POLLS: usize = 50;
pub const BOOTOUT_POLL_MS: u64 = 100;

/// The launchd agent label. One definition for the installer, the
/// uninstaller, and the target strings -- three spellings of this string
/// is how a label stops matching itself.
pub const LOGIN_AGENT_LABEL: &str = "com.antiknob.daemon";

// ---------------------------------------------------------------------------
// Config backups.
// ---------------------------------------------------------------------------

/// How many rotated `host.json.N` versions are kept. Older ones are
/// deleted, or the directory grows without bound.
pub const BACKUP_KEEP_COUNT: usize = 5;

// ---------------------------------------------------------------------------
// Diagnostics: patience budgets for human-driven probes.
// ---------------------------------------------------------------------------

/// Read timeout while sweeping raw payloads (`raw --read`), in
/// milliseconds. A query that answers nothing is a normal outcome when
/// sweeping, so the timeout ends the burst rather than failing.
pub const RAW_READ_TIMEOUT_MS: u64 = 250;

/// Poll interval while draining input reports during a gesture capture,
/// in milliseconds.
pub const CAPTURE_POLL_MS: u64 = 20;

/// Restore attempts for probe-armed slots before printing the put-it-back
/// commands. Failure after every attempt is reported with the exact
/// commands, never silently.
pub const PROBE_RESTORE_ATTEMPTS: u64 = 5;

/// Group range and counters for the exploratory `read-slots --wide` sweep.
/// The default bank the normal path reads lives in the firmware map; this
/// range is our own exploration choice, not a device property.
pub const WIDE_GROUP_FIRST: u8 = 0x00;
pub const WIDE_GROUP_LAST: u8 = 0x40;
pub const WIDE_COUNTERS: u8 = 8;
