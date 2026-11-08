//! What the slot-chord modifier hold actually costs (AK.HOLDMEASURE).
//!
//! `policy::MOD_HOLD_MS` is a guess that the knob's zero-delay chord chain
//! lands inside the hold. This module is the instrument that replaces the
//! guess with data: the grab tap reports, for every slot chord, the gap
//! between the chord's FIRST modifier press and its F-key, and every hold
//! that ran out with no chord. The daemon appends those to a small bounded
//! file; `antiknob hold-gaps` reads it back as a distribution.
//!
//! Privacy: a record is a slot ordinal (0..=4, a position in
//! `Gesture::ALL`) and a gap in microseconds, or a bare expiry marker. A
//! hold is only ever sampled for the five bound slot chords, never for a
//! key a person typed, and the keycode is not written at all -- so the file
//! cannot say what was typed, only how fast the knob's chords arrived.
//!
//! Bounded: the file never exceeds `policy::HOLD_LOG_CAP_BYTES`; past it the
//! oldest half is dropped, so a long-lived daemon keeps the recent past.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

/// One measurement the grab tap hands to the recorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldSample {
    /// A slot chord completed: first modifier press to F-key, microseconds.
    Gap { slot: u8, gap_us: u64 },
    /// A hold ran out with no chord to finish it.
    Expired,
}

impl HoldSample {
    /// The one line this sample occupies in the file.
    #[must_use]
    pub fn to_line(self) -> String {
        match self {
            Self::Gap { slot, gap_us } => format!("G {slot} {gap_us}\n"),
            Self::Expired => "X\n".to_string(),
        }
    }

    /// Inverse of `to_line`; a line that is not one of ours is `None`.
    #[must_use]
    pub fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.split_whitespace();
        match (parts.next()?, parts.next(), parts.next(), parts.next()) {
            ("G", Some(slot), Some(gap), None) => Some(Self::Gap {
                slot: slot.parse().ok()?,
                gap_us: gap.parse().ok()?,
            }),
            ("X", None, None, None) => Some(Self::Expired),
            _ => None,
        }
    }
}

/// Where the daemon writes and the CLI reads: beside the socket.
///
/// # Errors
/// `HOME` unset, or the state directory cannot be created.
pub fn default_path() -> anyhow::Result<PathBuf> {
    let socket = crate::api::socket::default_socket_path()?;
    let dir = socket
        .parent()
        .ok_or_else(|| anyhow::anyhow!("socket path has no parent directory"))?;
    Ok(dir.join(crate::policy::HOLD_LOG_FILE))
}

/// Append one sample, first dropping the oldest records if the file would
/// pass `cap` bytes. Never writes anything but `to_line` output.
///
/// # Errors
/// The file cannot be read, trimmed or appended to.
pub fn append(path: &Path, sample: HoldSample, cap: u64) -> std::io::Result<()> {
    let line = sample.to_line();
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    if size + line.len() as u64 > cap {
        trim_to(path, cap / 2)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// Keep the newest whole lines that fit in `keep` bytes.
fn trim_to(path: &Path, keep: u64) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut kept = 0usize;
    for line in text.split_inclusive('\n').rev() {
        if (kept + line.len()) as u64 > keep {
            break;
        }
        kept += line.len();
    }
    let start = text.len() - kept;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &text[start..])?;
    std::fs::rename(&tmp, path)
}

/// The distribution `antiknob hold-gaps` prints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summary {
    pub count: usize,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
    pub expired: usize,
}

/// Nearest-rank percentile of an ascending slice.
fn percentile(sorted: &[u64], pct: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (pct * sorted.len()).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

/// Summarise the file's text. Lines that are not ours are ignored.
#[must_use]
pub fn summarize(text: &str) -> Summary {
    let mut gaps = Vec::new();
    let mut expired = 0;
    for sample in text.lines().filter_map(HoldSample::from_line) {
        match sample {
            HoldSample::Gap { gap_us, .. } => gaps.push(gap_us),
            HoldSample::Expired => expired += 1,
        }
    }
    gaps.sort_unstable();
    Summary {
        count: gaps.len(),
        p50: percentile(&gaps, 50),
        p95: percentile(&gaps, 95),
        p99: percentile(&gaps, 99),
        max: gaps.last().copied(),
        expired,
    }
}

/// The printable table, with the hold the numbers are to be read against.
#[must_use]
pub fn render(s: &Summary) -> String {
    let us = |v: Option<u64>| v.map_or_else(|| "-".to_string(), |v| format!("{v} us"));
    format!(
        "slot chords       {}\n  p50             {}\n  p95             {}\n  p99             {}\n  max             {}\nexpired holds     {}\ncurrent hold      {} ms\n",
        s.count,
        us(s.p50),
        us(s.p95),
        us(s.p99),
        us(s.max),
        s.expired,
        crate::policy::MOD_HOLD_MS,
    )
}

#[cfg(test)]
#[path = "hold_stats_tests.rs"]
mod tests;
