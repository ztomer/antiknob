//! Every command this tool has, declared once.
//!
//! The CLI's clap definition and the MCP tool list are both GENERATED from
//! this table. They used to be two hand-written definitions, and they
//! drifted exactly as two hand-written definitions do: `bind-seq` shipped as
//! a CLI command with no MCP tool at all, `show-keys` was CLI-only, and
//! `read_slots`'s tool description still explained a model the burst read
//! had replaced. A parity test caught those, but a test that reports drift
//! is not the same as a structure that cannot drift.
//!
//! A command that is not on a surface says WHY, in the table, next to the
//! command. That is the difference between a considered omission and one
//! nobody has noticed yet.

/// What a parameter carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Presence-only; `--flag`.
    Flag,
    Str,
    Int,
    /// Repeated or comma-separated values.
    IntList,
    /// Free-form trailing words, e.g. an action sequence.
    StrList,
}

/// Which surfaces a parameter appears on.
///
/// Not every parameter can be shared: `upload` takes a FILE on the command
/// line and a YAML STRING over MCP, because an agent has no filesystem the
/// daemon can see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum On {
    Both,
    CliOnly,
    McpOnly,
}

#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub about: &'static str,
    pub required: bool,
    /// A positional argument on the CLI rather than `--name`.
    pub positional: bool,
    pub default: Option<&'static str>,
    pub on: On,
}

impl Param {
    pub const fn flag(name: &'static str, about: &'static str) -> Self {
        Self {
            name,
            kind: Kind::Flag,
            about,
            required: false,
            positional: false,
            default: None,
            on: On::Both,
        }
    }

    pub const fn opt(name: &'static str, kind: Kind, about: &'static str) -> Self {
        Self {
            name,
            kind,
            about,
            required: false,
            positional: false,
            default: None,
            on: On::Both,
        }
    }

    pub const fn req(name: &'static str, kind: Kind, about: &'static str) -> Self {
        Self {
            required: true,
            ..Self::opt(name, kind, about)
        }
    }

    pub const fn positional(mut self) -> Self {
        self.positional = true;
        self
    }

    pub const fn default(mut self, value: &'static str) -> Self {
        self.default = Some(value);
        self
    }

    pub const fn only(mut self, on: On) -> Self {
        self.on = on;
        self
    }

    pub fn on_surface(&self, mcp: bool) -> bool {
        match self.on {
            On::Both => true,
            On::CliOnly => !mcp,
            On::McpOnly => mcp,
        }
    }
}

/// Whether a command reaches a surface, and if not, why not.
#[derive(Debug, Clone, Copy)]
pub enum Reach {
    Yes,
    /// Absent on purpose. The string is the reason a reader can check.
    No(&'static str),
}

impl Reach {
    pub fn is_yes(&self) -> bool {
        matches!(self, Self::Yes)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// Canonical snake_case name. The CLI renders it kebab-case, MCP uses
    /// it as the tool name, so the two cannot be spelled differently.
    pub name: &'static str,
    pub about: &'static str,
    pub params: &'static [Param],
    /// The CLI spelling when it differs from the canonical name -- `status`
    /// rather than `get-status`. The names people type are older than this
    /// table and renaming them to satisfy it would be the tail wagging the
    /// dog.
    pub cli_as: Option<&'static str>,
    pub cli: Reach,
    pub mcp: Reach,
}

impl CommandSpec {
    /// The CLI spelling: the override if there is one, else the canonical
    /// name in kebab-case.
    pub fn cli_name(&self) -> String {
        self.cli_as
            .map_or_else(|| self.name.replace('_', "-"), str::to_string)
    }
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "get_status",
        about: "Probe and display status of connected Anticater / CH57x hardware without sudo.",
        params: &[Param::flag("json", "Machine-readable JSON device list")],
        cli_as: Some("status"),
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "validate",
        about: "Validate a keymap YAML layout offline, without touching the device.",
        params: &[Param::opt(
            "file",
            Kind::Str,
            "Layout file; defaults to the installed one",
        )
        .positional()],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::No("checks a local file; over MCP, upload_keymap reports the same errors"),
    },
    CommandSpec {
        name: "upload_keymap",
        about: "Flash a keymap to the device. A gesture may bind one action (\"volumeup\"), a \
                sequence ([\"cmd-c\", \"cmd-v\"]) or a timed one ({steps: [...], delay_ms: 120}). \
                Knob gestures are ccw, press, cw, hold_twist_l, hold_twist_r.",
        params: &[
            Param::opt("file", Kind::Str, "Layout file to flash")
                .positional()
                .only(On::CliOnly),
            Param::req("yaml", Kind::Str, "Layout YAML as a string").only(On::McpOnly),
            Param::opt("layer", Kind::Int, "Flash only this device layer"),
            Param::flag("no_verify", "Skip the post-flash read-back").only(On::CliOnly),
            Param::flag("knob_only", "Confirm the device really has no keys").only(On::CliOnly),
        ],
        cli_as: Some("upload"),
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "set_led",
        about: "Set a layer's backlight mode. Modes: off, red, green, ripple, rainbow, rgb.",
        params: &[
            Param::req("layer", Kind::Int, "Device layer 0-2").positional(),
            Param::req("mode", Kind::StrList, "Mode name, optionally with a colour").positional(),
        ],
        cli_as: Some("led"),
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "get_led",
        about: "Read back a layer's current LED mode. Read-only.",
        params: &[
            Param::req("layer", Kind::Int, "Device layer 0-2").positional(),
            Param::flag("raw", "Dump the full reply bytes").only(On::CliOnly),
        ],
        cli_as: Some("led-read"),
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "show_keys",
        about: "List every action name the device understands: keys, modifiers, media usages \
                and mouse actions. Call this rather than guessing a name -- an unknown name is \
                refused, and the vocabulary is not the same as a macOS key name.",
        params: &[],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "bind_slots",
        about: "Flash the one-time host-translate slot bindings (ctrl-alt-F16..F20, one per \
                gesture including hold+twist) so the daemon can translate knob gestures.",
        params: &[
            Param::opt("config", Kind::Str, "Layout to read the button count from")
                .only(On::CliOnly),
            Param::opt("buttons", Kind::Int, "Override the button count"),
            Param::opt("layer", Kind::Int, "Device layer to bind (default: all)"),
            Param::flag("dry_run", "Print the plan without touching hardware").only(On::CliOnly),
        ],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "bind_sequence",
        about: "Bind ONE knob gesture to a sequence of actions, with an optional wait between \
                steps. This is what lets a gesture type a string or drive an application. Key \
                ids: 2=twist CCW, 3=press, 4=twist CW, 5=hold+twist left, 6=hold+twist right. \
                A slot holds 19 entries and a chord costs one entry per modifier plus one for \
                the key. Keyboard actions chain; the firmware stores only ONE media action per \
                slot, so a media list is refused rather than silently truncated.",
        params: &[
            Param::req(
                "key",
                Kind::Int,
                "Slot key id: 2=CCW, 3=press, 4=CW, 5=hold L, 6=hold R",
            ),
            Param::opt("layer", Kind::Int, "Device layer 0-2").default("0"),
            Param::opt(
                "delay_ms",
                Kind::Int,
                "Milliseconds before each step after the first",
            )
            .default("0"),
            Param::opt("width", Kind::Int, "Width to read the table back at").only(On::CliOnly),
            Param::flag("dry_run", "Print the packet without writing").only(On::CliOnly),
            Param::req(
                "actions",
                Kind::StrList,
                "Action names in order, e.g. cmd-c cmd-v",
            )
            .positional(),
        ],
        cli_as: Some("bind-seq"),
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "read_slots",
        about: "Read the device's slot table. Pass full to get the WHOLE table in three burst \
                queries -- every key on every layer, including keys past whatever the declared \
                layout mentions.",
        params: &[
            Param::flag("full", "Read every key on every layer (recommended)"),
            Param::opt("config", Kind::Str, "Layout whose width to walk at").only(On::CliOnly),
            Param::flag("wide", "Exploratory sweep for protocol work").only(On::CliOnly),
            Param::opt("group", Kind::Int, "Per-slot form: the width").only(On::McpOnly),
            Param::opt("counters", Kind::IntList, "Per-slot form: which counters")
                .only(On::McpOnly),
        ],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "list_apps",
        about: "List installed macOS applications and their bundle IDs for launch/quit actions.",
        params: &[],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "send_raw",
        about: "Send a raw HID report payload to the device. For protocol work.",
        params: &[
            Param::flag("read", "Print the device's reply").only(On::CliOnly),
            Param::opt(
                "reads",
                Kind::Int,
                "How many replies to read; a query can burst",
            )
            .default("1")
            .only(On::CliOnly),
            Param::req("bytes", Kind::StrList, "Hex bytes, e.g. FD FE FF").positional(),
        ],
        cli_as: Some("raw"),
        cli: Reach::Yes,
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "import_presets",
        about: "Migrate the built-in presets to host-layer JSON for the daemon.",
        params: &[
            Param::opt("out", Kind::Str, "Write host.json here").only(On::CliOnly),
            Param::flag("force", "Overwrite an existing output file").only(On::CliOnly),
        ],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::No("a one-off migration of files on this machine, not a device operation"),
    },
    CommandSpec {
        name: "listen",
        about: "Dump raw input reports for a few seconds to see what the knob sends.",
        params: &[
            Param::opt(
                "device",
                Kind::StrList,
                "Only watch these devices, e.g. 514c:8850",
            ),
            Param::opt("timeout_secs", Kind::Int, "How long to listen").default("10"),
        ],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::No("streams for seconds and means nothing unless a human turns the knob"),
    },
    CommandSpec {
        name: "probe_gestures",
        about: "Find which firmware slot a gesture drives by writing a distinct marker to each \
                candidate and watching what comes out.",
        params: &[
            Param::opt("candidates", Kind::IntList, "Slots to probe").default("7,8,9,10,11"),
            Param::opt("control", Kind::Int, "Slot used as the positive control"),
            Param::opt("layer", Kind::Int, "Device layer to probe on").default("0"),
            Param::opt("capture_secs", Kind::Int, "Seconds to capture for").default("45"),
            Param::opt("device", Kind::StrList, "Only watch these devices"),
            Param::opt("config", Kind::Str, "Layout to read the layer width from"),
            Param::flag("map", "Map EVERY gesture at once across keys 1-6"),
        ],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::No("needs a human performing gestures inside the capture window"),
    },
    CommandSpec {
        name: "led_probe",
        about: "Walk every LED mode on one layer so an unmapped one can be identified by eye.",
        params: &[
            Param::opt("layer", Kind::Int, "Device layer to cycle")
                .default("0")
                .positional(),
            Param::opt("dwell_secs", Kind::Int, "Seconds to hold each mode").default("3"),
            Param::opt("color", Kind::Str, "Colour for modes that take one").default("red"),
        ],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::No("needs a human watching the LED to say which mode is which"),
    },
    CommandSpec {
        name: "mcp",
        about: "Run as an MCP server over stdio.",
        params: &[Param::opt("config", Kind::Str, "Host config JSON path")],
        cli_as: None,
        cli: Reach::Yes,
        mcp: Reach::No("starts the MCP server; exposing it over MCP is circular"),
    },
    // Daemon-state commands. They have no CLI form because they act on a
    // RUNNING daemon's in-memory engine, which a one-shot process does not
    // have -- the CLI would have to start a daemon to ask itself.
    CommandSpec {
        name: "get_config",
        about: "Return the daemon's current host-layer configuration.",
        params: &[],
        cli_as: None,
        cli: Reach::No("reads a running daemon's state; a one-shot CLI process has none"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "set_config",
        about: "Replace the daemon's host-layer configuration.",
        params: &[Param::req("config", Kind::Str, "Full host config JSON")],
        cli_as: None,
        cli: Reach::No("writes a running daemon's state; a one-shot CLI process has none"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "set_layer",
        about: "Switch the daemon's active host layer.",
        params: &[Param::req("index", Kind::Int, "Layer index")],
        cli_as: None,
        cli: Reach::No("acts on a running daemon's engine"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "get_virtual_layer",
        about: "Report the virtual layer: its variants, which is live and why, and which \
                device layer the daemon can actually hear.",
        params: &[],
        cli_as: None,
        cli: Reach::No("acts on a running daemon's engine"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "set_virtual_variant",
        about: "Pin the virtual layer to a named variant, or clear the pin.",
        params: &[Param::opt(
            "variant",
            Kind::Str,
            "Variant name; omit to clear the pin",
        )],
        cli_as: None,
        cli: Reach::No("acts on a running daemon's engine"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "get_knob_mode",
        about: "Report whether the firmware sends the bound slot chords, and which device \
                layer carries them.",
        params: &[Param::opt(
            "buttons",
            Kind::Int,
            "Override the button count",
        )],
        cli_as: None,
        cli: Reach::No("the CLI reports the same thing through get_status"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "list_commands",
        about: "Describe every command this build exposes, with its parameters and which \
                surfaces carry it. Read from the same table that generates the CLI and the \
                MCP tool list, so a caller cannot be told about a command that is not here.",
        params: &[],
        cli_as: None,
        cli: Reach::No("the CLI's own --help renders this table already"),
        mcp: Reach::Yes,
    },
    CommandSpec {
        name: "ping",
        about: "Application-level liveness probe confirming the daemon is responsive.",
        params: &[],
        cli_as: None,
        cli: Reach::No("a daemon liveness probe; a CLI process that runs has already answered"),
        mcp: Reach::Yes,
    },
];

/// The commands one surface exposes.
pub fn for_surface(mcp: bool) -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS
        .iter()
        .filter(move |c| if mcp { c.mcp.is_yes() } else { c.cli.is_yes() })
}
