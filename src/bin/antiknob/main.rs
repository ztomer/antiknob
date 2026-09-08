//! `antiknob` -- the command line surface.
//!
//! The subcommands and their arguments are GENERATED from
//! `antiknob::api::registry::COMMANDS`, the same table the MCP tool list is
//! generated from. There is no enum of commands here to keep in step with
//! the tool definitions, because keeping two definitions in step is a thing
//! nobody does reliably: `bind-seq` existed here for a day with no MCP tool
//! at all, and `show-keys` never had one.
//!
//! What remains is the dispatch: turning parsed arguments into a call. That
//! is genuinely per-command work and cannot be generated, but a command
//! cannot reach it without being in the registry, and a registry entry with
//! no arm here is a named runtime refusal rather than a silent no-op.

use std::path::PathBuf;

use antiknob::api::registry::{self, Kind};
use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};

mod binding;
mod cmds;
mod diag;
mod probe;

/// Build the whole CLI from the registry.
fn cli() -> Command {
    let mut app = Command::new("antiknob")
        .version(env!("CARGO_PKG_VERSION"))
        .about(
            "Native macOS Apple Silicon configurator for Anticater VK01 Knob (MIT OR Apache-2.0)",
        )
        .subcommand_required(true)
        .arg_required_else_help(true);

    for spec in registry::for_surface(false) {
        let name: &'static str = Box::leak(spec.cli_name().into_boxed_str());
        let mut sub = Command::new(name).about(spec.about);
        for param in spec.params.iter().filter(|p| p.on_surface(false)) {
            sub = sub.arg(build_arg(param));
        }
        app = app.subcommand(sub);
    }
    app
}

/// One registry parameter as a clap argument.
fn build_arg(param: &registry::Param) -> Arg {
    let mut arg = Arg::new(param.name).help(param.about);

    if !param.positional {
        // `no_verify` is spelled `--no-verify`; the registry keeps the
        // Rust-side name so the extraction below can use it verbatim.
        let long: &'static str = Box::leak(param.name.replace('_', "-").into_boxed_str());
        arg = arg.long(long);
    }

    arg = match param.kind {
        Kind::Flag => arg.action(ArgAction::SetTrue),
        Kind::Str | Kind::Int => arg.action(ArgAction::Set),
        // Comma-separated so `--candidates 7,8,9` works, and repeatable.
        Kind::IntList => arg.action(ArgAction::Append).value_delimiter(','),
        // Everything left on the line: an action sequence, a raw payload, an
        // LED mode with a colour. Hyphens are values here, not flags, or
        // `led 0 static white` could not name a chord like `cmd-c`.
        Kind::StrList => arg
            .action(ArgAction::Append)
            .num_args(1..)
            .allow_hyphen_values(true),
    };

    if param.required {
        arg = arg.required(true);
    }
    if let Some(default) = param.default {
        arg = arg.default_value(default);
    }
    arg
}

// ---- typed extraction -------------------------------------------------
//
// clap hands back strings; these turn them into what the handlers take. A
// parse failure names the argument rather than defaulting to zero.

fn flag(m: &ArgMatches, name: &str) -> bool {
    m.get_flag(name)
}

fn opt_str(m: &ArgMatches, name: &str) -> Option<String> {
    m.get_one::<String>(name).cloned()
}

fn opt_path(m: &ArgMatches, name: &str) -> Option<PathBuf> {
    opt_str(m, name).map(PathBuf::from)
}

fn opt_num<T: std::str::FromStr>(m: &ArgMatches, name: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    match m.get_one::<String>(name) {
        None => Ok(None),
        Some(raw) => raw.parse::<T>().map(Some).map_err(|e| {
            anyhow::anyhow!("{}: '{}' is not valid ({e})", name.replace('_', "-"), raw)
        }),
    }
}

fn num<T: std::str::FromStr + Default>(m: &ArgMatches, name: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    Ok(opt_num::<T>(m, name)?.unwrap_or_default())
}

fn strings(m: &ArgMatches, name: &str) -> Vec<String> {
    m.get_many::<String>(name)
        .map(|v| v.cloned().collect())
        .unwrap_or_default()
}

fn nums<T: std::str::FromStr>(m: &ArgMatches, name: &str) -> Result<Vec<T>>
where
    T::Err: std::fmt::Display,
{
    strings(m, name)
        .iter()
        .map(|raw| {
            raw.parse::<T>()
                .map_err(|e| anyhow::anyhow!("{name}: '{raw}' is not valid ({e})"))
        })
        .collect()
}

fn main() -> Result<()> {
    let matches = cli().get_matches();
    let (name, m) = matches.subcommand().expect("a subcommand is required");

    // Dispatch on the registry's CANONICAL name, so an arm and a table entry
    // cannot disagree about which command this is.
    let spec = registry::for_surface(false)
        .find(|s| s.cli_name() == name)
        .expect("clap only offers subcommands the registry defines");

    match spec.name {
        "get_status" => cmds::run_status(flag(m, "json"))?,
        "validate" => cmds::run_validate(opt_path(m, "file"))?,
        "upload_keymap" => cmds::run_upload(
            opt_path(m, "file"),
            opt_num::<u8>(m, "layer")?,
            flag(m, "no_verify"),
            flag(m, "knob_only"),
        )?,
        "set_led" => cmds::run_led(num::<u8>(m, "layer")?, strings(m, "mode"))?,
        "get_led" => cmds::run_led_read(num::<u8>(m, "layer")?, flag(m, "raw"))?,
        "show_keys" => cmds::run_show_keys()?,
        "bind_slots" => binding::run_bind_slots(
            opt_path(m, "config"),
            opt_num::<usize>(m, "buttons")?,
            opt_num::<u8>(m, "layer")?,
            flag(m, "dry_run"),
        )?,
        "bind_sequence" => binding::run_bind_seq(
            num::<u8>(m, "key")?,
            num::<u8>(m, "layer")?,
            opt_num::<u8>(m, "width")?,
            flag(m, "dry_run"),
            num::<u16>(m, "delay_ms")?,
            strings(m, "actions"),
        )?,
        "listen" => diag::run_listen(num::<u64>(m, "timeout_secs")?, strings(m, "device"))?,
        "import_presets" => cmds::run_import_presets(opt_path(m, "out"), flag(m, "force"))?,
        "list_apps" => cmds::run_list_apps()?,
        "read_slots" => {
            if flag(m, "full") {
                diag::run_read_full()?
            } else {
                diag::run_read_slots(
                    cmds::layout_slots_per_layer(opt_path(m, "config"))?,
                    flag(m, "wide"),
                )?
            }
        }
        "probe_gestures" => {
            let config = opt_path(m, "config");
            let candidates = nums::<u8>(m, "candidates")?;
            let layer = num::<u8>(m, "layer")?;
            let capture_secs = num::<u64>(m, "capture_secs")?;
            let devices = strings(m, "device");
            if flag(m, "map") {
                return probe::run_map((1..=6).collect(), layer, capture_secs, devices);
            }
            // Widen to the highest candidate so the read can address it: the
            // device only walks a table as wide as it is told, and the
            // candidates deliberately sit past the declared layout.
            let width = cmds::layout_slots_per_layer(config.clone())?
                .max(candidates.iter().copied().max().unwrap_or(0));
            // The knob's CCW slot follows the buttons, so it is the first
            // slot past them -- the same arithmetic `key_id_for_knob` does.
            let control = match opt_num::<u8>(m, "control")? {
                Some(c) => c,
                None => antiknob::protocol::key_id_for_knob(
                    binding::button_count_for(config)?,
                    0,
                    antiknob::protocol::KnobEvent::RotateCCW,
                ),
            };
            probe::run(control, candidates, layer, width, capture_secs, devices)?
        }
        "led_probe" => diag::run_led_probe(
            num::<u8>(m, "layer")?,
            num::<u64>(m, "dwell_secs")?,
            &opt_str(m, "color").unwrap_or_else(|| "red".into()),
        )?,
        "send_raw" => diag::run_raw(
            strings(m, "bytes"),
            flag(m, "read"),
            num::<usize>(m, "reads")?,
        )?,
        "mcp" => {
            let config_path = match opt_path(m, "config") {
                Some(p) => p,
                None => antiknob::host::default_config_path()?,
            };
            let ctx = antiknob::api::ApiContext::new(config_path, None);
            antiknob::api::run_mcp_server(ctx)?;
        }
        other => anyhow::bail!(
            "the registry offers '{other}' but nothing here handles it; add an arm in main.rs"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use antiknob::api::registry::Reach;

    /// clap validates its own definition; a malformed argument becomes a
    /// test failure rather than a surprise for whoever runs the command.
    #[test]
    fn the_generated_cli_is_well_formed() {
        cli().debug_assert();
    }

    /// Every registry command the CLI claims to expose must actually render
    /// as a subcommand. Generation is the whole point, so one that silently
    /// failed to appear would defeat it.
    #[test]
    fn every_cli_command_in_the_registry_is_offered() {
        let app = cli();
        let offered: Vec<String> = app
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect();
        for spec in registry::COMMANDS.iter().filter(|s| s.cli.is_yes()) {
            assert!(
                offered.contains(&spec.cli_name()),
                "{} is missing from the CLI: {offered:?}",
                spec.cli_name()
            );
        }
        assert_eq!(offered.len(), registry::for_surface(false).count());
    }

    /// Every command reaches a handler. Without this, adding a registry
    /// entry and forgetting the arm is a runtime error nobody meets until
    /// they run that exact command.
    #[test]
    fn every_offered_command_has_a_handler() {
        // The arm list is the match's own, kept beside it so the two are
        // read together.
        const HANDLED: &[&str] = &[
            "get_status",
            "validate",
            "upload_keymap",
            "set_led",
            "get_led",
            "show_keys",
            "bind_slots",
            "bind_sequence",
            "listen",
            "import_presets",
            "list_apps",
            "read_slots",
            "probe_gestures",
            "led_probe",
            "send_raw",
            "mcp",
        ];
        for spec in registry::for_surface(false) {
            assert!(
                HANDLED.contains(&spec.name),
                "'{}' is offered by the CLI but has no arm in main.rs",
                spec.name
            );
        }
    }

    /// A command a surface withholds must say why, in a sentence a reader
    /// can check rather than a placeholder.
    #[test]
    fn a_withheld_command_gives_a_real_reason() {
        for spec in registry::COMMANDS {
            for reach in [&spec.cli, &spec.mcp] {
                if let Reach::No(reason) = reach {
                    assert!(
                        reason.len() > 20,
                        "{} is withheld with a reason too thin to check: {reason:?}",
                        spec.name
                    );
                }
            }
        }
    }
}
