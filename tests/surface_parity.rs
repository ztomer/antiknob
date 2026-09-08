//! The CLI and the MCP tool list must describe the same device.
//!
//! They drifted. `bind-seq` -- the whole point of the `0xFD` work -- existed
//! as a CLI command for a day with no MCP tool, so an agent could not bind a
//! sequence at all. `show-keys` was CLI-only, leaving an agent to guess
//! action names against a parser that refuses unknown ones. And
//! `read_slots`'s MCP description still explained a per-slot model that the
//! burst read had already replaced.
//!
//! All three were found by reading two lists side by side, which is exactly
//! the review that does not happen twice. So the parity is a test.
//!
//! The vocabulary gets the same treatment: every name the tables list must
//! actually parse, because the CLI's `show-keys` used to be a hand-written
//! string that had already fallen behind the parser.

use antiknob::api::types::all_tools;
use antiknob::protocol::Action;
use antiknob::vocabulary::{KEY_NAMES, MEDIA_NAMES, MODIFIER_NAMES, MOUSE_NAMES};

/// CLI commands with no MCP tool, each with the reason it cannot have one.
/// Anything not listed here MUST be exposed. Shrink this list; never grow it
/// without a reason a reader can check.
const CLI_ONLY: &[(&str, &str)] = &[
    (
        "mcp",
        "starts the MCP server; exposing it over MCP is circular",
    ),
    ("help", "clap's own, not a device operation"),
    (
        "validate",
        "checks a local file; an agent can call upload_keymap and read the error",
    ),
    (
        "import-presets",
        "one-off migration of the built-in presets",
    ),
    (
        "listen",
        "streams input reports for seconds; needs a human turning the knob",
    ),
    (
        "probe-gestures",
        "needs a human performing gestures inside a capture window",
    ),
    (
        "led-probe",
        "needs a human watching the LED to say which mode is which",
    ),
    ("show-keys", "exposed as the MCP tool `show_keys`"),
    ("bind-seq", "exposed as the MCP tool `bind_sequence`"),
    ("read-slots", "exposed as the MCP tool `read_slots`"),
    ("bind-slots", "exposed as the MCP tool `bind_slots`"),
    ("led-read", "exposed as the MCP tool `get_led`"),
    ("upload", "exposed as the MCP tool `upload_keymap`"),
    ("list-apps", "exposed as the MCP tool `list_apps`"),
    ("status", "exposed as the MCP tool `get_status`"),
    ("led", "exposed as the MCP tool `set_led`"),
    ("raw", "exposed as the MCP tool `send_raw`"),
];

fn cli_commands() -> Vec<String> {
    // Parsed from the built binary's own help, so this cannot drift from
    // what clap actually offers.
    let exe = env!("CARGO_BIN_EXE_antiknob");
    let out = std::process::Command::new(exe)
        .arg("--help")
        .output()
        .expect("run antiknob --help");
    let text = String::from_utf8_lossy(&out.stdout);
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in text.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.starts_with("Options:") {
                break;
            }
            if let Some(first) = line.split_whitespace().next() {
                if line.starts_with("  ") && !first.starts_with('-') {
                    names.push(first.to_string());
                }
            }
        }
    }
    assert!(names.len() > 10, "parsed too few commands: {names:?}");
    names
}

#[test]
fn every_cli_command_is_reachable_over_mcp_or_says_why_not() {
    let tools: Vec<&str> = all_tools().iter().map(|t| t.name).collect();
    let mut unexplained = Vec::new();
    for cmd in cli_commands() {
        // `foo-bar` on the CLI is `foo_bar` over MCP.
        let mcp = cmd.replace('-', "_");
        if tools.contains(&mcp.as_str()) {
            continue;
        }
        if CLI_ONLY.iter().any(|(name, _)| *name == cmd) {
            continue;
        }
        unexplained.push(cmd);
    }
    assert!(
        unexplained.is_empty(),
        "CLI commands with no MCP tool and no stated reason: {unexplained:?}. \
         Either add the tool or add it to CLI_ONLY with a reason."
    );
}

/// A stale exemption is worse than none: it reads as a considered decision
/// while describing a command that no longer exists.
#[test]
fn the_exemption_list_names_only_real_commands() {
    let commands = cli_commands();
    for (name, reason) in CLI_ONLY {
        assert!(
            commands.contains(&name.to_string()),
            "CLI_ONLY names '{name}' ({reason}), which is not a command any more"
        );
    }
}

#[test]
fn every_mcp_tool_has_a_description_that_says_something() {
    for tool in all_tools() {
        assert!(
            tool.description.len() > 40,
            "{} has a description too short to guide a caller: {:?}",
            tool.name,
            tool.description
        );
        assert!(tool.input_schema.is_object(), "{} has no schema", tool.name);
    }
}

/// The vocabulary is one table read by the parser and rendered by both
/// surfaces. If a name is listed, it must parse -- the hand-written list
/// this replaced had fallen behind the parser without anyone noticing.
#[test]
fn every_listed_action_name_actually_parses() {
    for (name, _) in KEY_NAMES {
        Action::parse(name).unwrap_or_else(|e| panic!("key '{name}' does not parse: {e}"));
    }
    for (name, _) in MEDIA_NAMES {
        Action::parse(name).unwrap_or_else(|e| panic!("media '{name}' does not parse: {e}"));
    }
    for name in MOUSE_NAMES {
        Action::parse(name).unwrap_or_else(|e| panic!("mouse '{name}' does not parse: {e}"));
    }
    for (name, _) in MODIFIER_NAMES {
        let chord = format!("{name}-a");
        Action::parse(&chord).unwrap_or_else(|e| panic!("modifier '{name}' does not parse: {e}"));
    }
}

/// And the tables must cover what the parser accepts, not just be covered by
/// it -- a key that parses but is unlisted is unreachable to anyone reading
/// `show-keys` or calling `show_keys`.
#[test]
fn the_tables_carry_the_names_this_session_added() {
    for name in ["fastforward", "rewind", "eject"] {
        assert!(
            MEDIA_NAMES.iter().any(|(n, _)| *n == name),
            "'{name}' parses but is not listed, so nothing reading the help can find it"
        );
    }
    for name in ["f24", "brightnessup"] {
        let listed = KEY_NAMES.iter().any(|(n, _)| *n == name)
            || MEDIA_NAMES.iter().any(|(n, _)| *n == name);
        assert!(listed, "'{name}' is missing from the tables");
    }
}
