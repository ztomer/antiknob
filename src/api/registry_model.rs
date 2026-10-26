//! The vocabulary the command table is written in: what a parameter carries,
//! where it appears, and what a command does to the world. Split from
//! `registry.rs` so the table itself stays under the file-length cap.

/// What a parameter carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Presence-only; `--flag`.
    Flag,
    Str,
    Int,
    /// A JSON object, passed structurally over MCP (never on the CLI).
    Object,
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

/// What running a command does to the world, in the terms an MCP host's
/// permission prompt needs (`readOnlyHint` / `destructiveHint`).
///
/// Declared here, beside the command, so the annotation a client sees cannot
/// drift from the command it describes -- and so a new command must say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Changes nothing.
    Read,
    /// Changes live or easily restored state (the backlight, the active
    /// host layer); never overwrites anything that cannot be set back.
    Write,
    /// Overwrites the device's stored bindings, a config file, or sends
    /// arbitrary bytes to the firmware. Not offered over MCP unless the
    /// server was started with `--allow-destructive`.
    Destructive,
}

/// One top-level field of a command's reply: its name and JSON type.
pub type Ret = (&'static str, &'static str);

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
    pub effect: Effect,
    /// The fields every successful reply carries, with their JSON types --
    /// the MCP `outputSchema`. Only fields that are always present are
    /// listed; a reply may carry more.
    pub returns: &'static [Ret],
}

impl CommandSpec {
    /// The CLI spelling: the override if there is one, else the canonical
    /// name in kebab-case.
    pub fn cli_name(&self) -> String {
        self.cli_as
            .map_or_else(|| self.name.replace('_', "-"), str::to_string)
    }
}
