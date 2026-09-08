//! The command table, rendered for a surface that wants to show it.
//!
//! Split from `dispatch.rs` along a real seam: nothing here executes a
//! command or touches a device. It reads `registry::COMMANDS` -- the table
//! that already generates the CLI's clap tree and the MCP tool list -- and
//! turns it into JSON, so a third surface can render the same declaration
//! rather than keeping a fourth hand-written copy of it. The settings app
//! kept exactly such a copy, and it had drifted to eleven entries against
//! seventeen.

use serde_json::{json, Value};

/// The command table as JSON, for any surface that wants to render it.
///
/// One shape per command: what it is called, what it does, which surfaces
/// carry it (and the stated reason when one does not), and its parameters as
/// they appear on THIS surface. The settings app used to carry its own copy
/// of this list; two hand-written lists drift, and that one had.
pub fn describe_commands() -> Vec<Value> {
    use crate::api::registry::{Kind, Reach, COMMANDS};

    fn kind_name(kind: Kind) -> &'static str {
        match kind {
            Kind::Flag => "flag",
            Kind::Str => "string",
            Kind::Int => "integer",
            Kind::IntList => "integer[]",
            Kind::StrList => "string[]",
        }
    }

    fn reach(r: Reach) -> Value {
        match r {
            Reach::Yes => json!({ "available": true }),
            Reach::No(why) => json!({ "available": false, "reason": why }),
        }
    }

    COMMANDS
        .iter()
        .map(|c| {
            // The socket speaks the same `Command` enum as MCP, so the
            // MCP-facing parameter view is the one a socket caller sends.
            let params: Vec<Value> = c
                .params
                .iter()
                .filter(|p| p.on_surface(true))
                .map(|p| {
                    json!({
                        "name": p.name,
                        "kind": kind_name(p.kind),
                        "about": p.about,
                        "required": p.required,
                    })
                })
                .collect();
            json!({
                "name": c.name,
                "about": c.about,
                "cli_name": c.cli_name(),
                "cli": reach(c.cli),
                "mcp": reach(c.mcp),
                "params": params,
            })
        })
        .collect()
}
