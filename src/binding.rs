//! What one gesture does.
//!
//! Split from `config.rs` for the file-length gate, along a real seam: this
//! is the vocabulary a layout uses to describe a gesture, and it is the one
//! place that decides WHICH firmware command a binding needs. `config.rs`
//! keeps the layout around it.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// What one gesture does: a single action, or a sequence.
///
/// A single action keeps the `0xFE` path it has always used -- that one is
/// verified for keyboard, media AND mouse. A sequence needs the vendor's
/// `0xFD` command, which is the only one with a field for more than one
/// action; see `crate::fd`.
///
/// ```yaml
/// ccw: "volumedown"                 # one action, as before
/// press: ["cmd-c", "cmd-v"]         # a sequence, no waiting
/// cw:                               # a sequence that waits between steps
///   steps: ["cmd-a", "cmd-c"]
///   delay_ms: 120
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Binding {
    One(String),
    Sequence(Vec<String>),
    Timed {
        steps: Vec<String>,
        #[serde(default)]
        delay_ms: u16,
    },
}

impl Binding {
    /// The action names, in order.
    pub fn names(&self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s.clone()],
            Self::Sequence(v) => v.clone(),
            Self::Timed { steps, .. } => steps.clone(),
        }
    }

    /// True when this needs the `0xFD` sequence writer rather than `0xFE`.
    ///
    /// A one-element list or an explicit `delay_ms` still counts: the user
    /// asked for the sequence form, and silently downgrading it would make
    /// `steps: [x]` and `x` behave differently from how they read.
    pub fn is_sequence(&self) -> bool {
        !matches!(self, Self::One(_))
    }

    pub fn delay_ms(&self) -> u16 {
        match self {
            Self::Timed { delay_ms, .. } => *delay_ms,
            Self::One(_) | Self::Sequence(_) => 0,
        }
    }

    /// The steps this binding flashes, with the delay on every step after
    /// the first -- waiting before the opening keystroke only adds lag.
    pub fn steps(&self) -> Result<Vec<crate::fd::Step>> {
        let mut steps = crate::fd::parse_sequence(&self.names())?;
        for step in steps.iter_mut().skip(1) {
            step.delay_ms = self.delay_ms();
        }
        Ok(steps)
    }

    /// The packet that flashes this binding to one slot.
    ///
    /// A single action goes out as `0xFE`, the path verified for keyboard,
    /// media and mouse alike. A sequence goes out as `0xFD`, the only
    /// command with a field for more than one action. One helper so the CLI
    /// and the daemon cannot disagree about which command a binding uses.
    pub fn to_packet(&self, key_id: u8, layer: u8) -> Result<Vec<u8>> {
        match self {
            Self::One(name) => Ok(crate::protocol::Action::parse(name)?.to_packet(key_id, layer)),
            _ => crate::fd::build_packet(key_id, layer, &self.steps()?),
        }
    }

    /// Refuse anything the device or the writer cannot store, at load time
    /// rather than at flash time.
    pub fn validate(&self) -> Result<()> {
        if self.names().is_empty() {
            anyhow::bail!("a binding with no actions does nothing; remove it or give it an action");
        }
        for name in self.names() {
            crate::protocol::Action::parse(&name)?;
        }
        if self.is_sequence() {
            // Building the packet is the check: it knows the entry budget,
            // that a chord costs one entry per modifier, that media cannot
            // chain and that mouse has no measured encoding.
            crate::fd::build_packet(1, 0, &self.steps()?)?;
        }
        Ok(())
    }
}
