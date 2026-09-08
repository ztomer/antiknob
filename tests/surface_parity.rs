//! The CLI and the MCP tool list are generated from one table. These check
//! the generation, and that the table's omissions are deliberate.
//!
//! Before the registry, both surfaces were hand-written a thousand lines
//! apart and drifted exactly as you would expect: `bind-seq` shipped as a
//! CLI command with no MCP tool, so an agent could not bind a sequence at
//! all; `show-keys` was CLI-only, leaving one to guess action names against
//! a parser that refuses unknown ones; and `read_slots`'s tool description
//! still explained a per-slot model the burst read had replaced. A parity
//! test found those. This file no longer needs to: a command cannot exist
//! on one surface and not the other unless the table says so, in a sentence.

use antiknob::api::registry::{self, Kind, On, Reach};
use antiknob::api::types::all_tools;
use antiknob::protocol::Action;
use antiknob::vocabulary::{KEY_NAMES, MEDIA_NAMES, MODIFIER_NAMES, MOUSE_NAMES};

#[test]
fn the_mcp_tool_list_is_exactly_what_the_registry_exposes() {
    let tools: Vec<&str> = all_tools().iter().map(|t| t.name).collect();
    let expected: Vec<&str> = registry::for_surface(true).map(|s| s.name).collect();
    assert_eq!(tools, expected);
    assert!(!tools.is_empty());
}

/// A tool's schema comes from the table, so this checks the GENERATION:
/// required params are marked required, and CLI-only ones stay out.
#[test]
fn generated_schemas_carry_required_params_and_omit_cli_only_ones() {
    let tools = all_tools();
    let find = |name: &str| tools.iter().find(|t| t.name == name).expect(name).clone();

    let bind = find("bind_sequence");
    let required = bind.input_schema["required"].as_array().expect("required");
    assert!(required.iter().any(|v| v == "key"));
    assert!(required.iter().any(|v| v == "actions"));
    let props = bind.input_schema["properties"]
        .as_object()
        .expect("properties");
    assert!(props.contains_key("delay_ms"));
    // `--dry-run` and `--width` are CLI conveniences, not part of the tool.
    assert!(
        !props.contains_key("dry_run"),
        "cli-only param leaked into MCP"
    );
    assert!(
        !props.contains_key("width"),
        "cli-only param leaked into MCP"
    );

    // upload takes a FILE on the CLI and YAML over MCP; only one may appear.
    let upload = find("upload_keymap");
    let props = upload.input_schema["properties"]
        .as_object()
        .expect("properties");
    assert!(props.contains_key("yaml"));
    assert!(
        !props.contains_key("file"),
        "a CLI path leaked into the tool schema"
    );
}

/// Every omission is a decision someone wrote down, not an oversight.
#[test]
fn a_command_missing_from_a_surface_says_why() {
    let mut withheld = 0;
    for spec in registry::COMMANDS {
        for (surface, reach) in [("cli", &spec.cli), ("mcp", &spec.mcp)] {
            if let Reach::No(reason) = reach {
                withheld += 1;
                assert!(
                    reason.len() > 20,
                    "{} is withheld from {surface} with a reason too thin to check: {reason:?}",
                    spec.name
                );
            }
        }
        assert!(
            spec.cli.is_yes() || spec.mcp.is_yes(),
            "{} reaches no surface at all",
            spec.name
        );
    }
    assert!(withheld > 0, "the reasons are not being exercised");
}

/// A parameter that appears on neither surface is dead weight that reads
/// like configuration.
#[test]
fn every_parameter_reaches_a_surface() {
    for spec in registry::COMMANDS {
        for param in spec.params {
            let reachable = (param.on_surface(false) && spec.cli.is_yes())
                || (param.on_surface(true) && spec.mcp.is_yes());
            assert!(
                reachable,
                "{}.{} is on no surface anyone can call",
                spec.name, param.name
            );
        }
    }
}

/// A required flag is a contradiction: a flag is present or absent, so
/// requiring one means it can only ever be true.
#[test]
fn no_flag_is_marked_required() {
    for spec in registry::COMMANDS {
        for param in spec.params {
            assert!(
                !(param.kind == Kind::Flag && param.required),
                "{}.{} is a required flag, which can only ever be true",
                spec.name,
                param.name
            );
        }
    }
}

/// An MCP-only parameter on a command MCP does not expose, or a CLI-only
/// one on a command the CLI does not expose, is a contradiction.
#[test]
fn surface_only_parameters_match_their_commands_reach() {
    for spec in registry::COMMANDS {
        for param in spec.params {
            if param.on == On::McpOnly {
                assert!(
                    spec.mcp.is_yes(),
                    "{}.{} is MCP-only on a command MCP does not expose",
                    spec.name,
                    param.name
                );
            }
            if param.on == On::CliOnly {
                assert!(
                    spec.cli.is_yes(),
                    "{}.{} is CLI-only on a command the CLI does not expose",
                    spec.name,
                    param.name
                );
            }
        }
    }
}

/// The vocabulary is one table read by the parser and rendered by both
/// surfaces. If a name is listed it must parse -- the hand-written list this
/// replaced had fallen behind the parser without anyone noticing.
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

/// And the tables must cover what the parser accepts, not merely be covered
/// by it: a name that parses but is unlisted is unreachable to anyone
/// reading `show-keys` or calling `show_keys`.
#[test]
fn the_tables_carry_the_names_this_session_added() {
    for name in ["fastforward", "rewind", "eject"] {
        assert!(
            MEDIA_NAMES.iter().any(|(n, _)| *n == name),
            "'{name}' parses but is not listed, so nothing reading the help can find it"
        );
    }
}
