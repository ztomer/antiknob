use anyhow::{Context, Result};

pub use crate::binding::Binding;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnobConfig {
    #[serde(default)]
    pub ccw: Option<Binding>,
    #[serde(default)]
    pub press: Option<Binding>,
    #[serde(default)]
    pub cw: Option<Binding>,
    /// Hold the knob down and twist left. Key 5 on a VK01, measured with
    /// `probe-gestures --map`. This repo said for months that the gesture
    /// did not exist, then that its slot was unknown. It is neither.
    #[serde(default, alias = "holdTwistL")]
    pub hold_twist_l: Option<Binding>,
    /// Hold the knob down and twist right. Key 6.
    #[serde(default, alias = "holdTwistR")]
    pub hold_twist_r: Option<Binding>,
}

impl KnobConfig {
    /// Every gesture this knob binds, paired with the event it drives.
    ///
    /// One list, so a caller cannot handle three gestures and forget the
    /// other two -- which is how hold+twist stayed unreachable even after
    /// its slots were known.
    pub fn bindings(&self) -> Vec<(crate::protocol::KnobEvent, &Binding)> {
        use crate::protocol::KnobEvent as E;
        [
            (E::RotateCCW, &self.ccw),
            (E::Press, &self.press),
            (E::RotateCW, &self.cw),
            (E::HoldTwistL, &self.hold_twist_l),
            (E::HoldTwistR, &self.hold_twist_r),
        ]
        .into_iter()
        .filter_map(|(event, slot)| slot.as_ref().map(|b| (event, b)))
        .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    #[serde(default)]
    pub buttons: Vec<Vec<String>>,
    #[serde(default)]
    pub knobs: Vec<KnobConfig>,
    #[serde(default)]
    pub led: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_orientation")]
    pub orientation: String,
    #[serde(default)]
    pub rows: usize,
    #[serde(default)]
    pub columns: usize,
    #[serde(default = "default_knobs")]
    pub knobs: usize,
    pub layers: Vec<LayerConfig>,
}

fn default_model() -> String {
    "ch57x-1".to_string()
}

fn default_orientation() -> String {
    "normal".to_string()
}

fn default_knobs() -> usize {
    1
}

/// The packaged starter layout, seeded on first use.
///
/// Compiled in rather than installed beside the binaries: a copy under
/// `/Applications` was never a config location -- nothing resolved it, so it
/// only worked if the user happened to `cd` there first.
pub const STARTER_CONFIG: &str = include_str!("../config.yaml");

/// Where the hardware layout lives when no path is given.
///
/// Alongside `host.json` and the socket in Application Support, which is the
/// macOS convention and the only directory the daemon already owns. The old
/// default was the literal relative path `config.yaml`, so every command
/// that took one worked from exactly one directory.
pub fn default_device_config_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/antiknob/config.yaml")
}

/// Resolve the layout path, seeding the starter file when nothing is there.
///
/// Never overwrites: an existing file is the user's, however old.
pub fn resolve_device_config_path(home: &Path, explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let path = default_device_config_path(home);
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, STARTER_CONFIG)?;
    }
    Ok(path)
}

impl DeviceConfig {
    /// How many physical buttons the declared layout has.
    ///
    /// The one place that answer comes from. Knob slot IDs continue the
    /// key-ID space after the buttons, so a guessed count writes bindings
    /// into slots the firmware never reads and reports nothing wrong --
    /// which is exactly the defect `protocol::key_id_for_knob` documents.
    pub fn button_count(&self) -> usize {
        self.rows * self.columns
    }

    /// Total slots one layer holds: every button, then three per knob.
    ///
    /// This is `read_slot`'s first parameter -- the device wants to be told
    /// how wide a layer is before it will walk the table.
    pub fn slots_per_layer(&self) -> usize {
        self.button_count() + self.knobs * crate::protocol::GESTURES_PER_KNOB
    }

    /// True when this layout puts knob slots at key IDs 1/2/3.
    ///
    /// Harmless on a device with no keys and destructive on one with them:
    /// the knob bindings land exactly on the button slots. The config cannot
    /// tell which device it will be flashed to, so the caller has to say --
    /// `upload --knob-only`. Before knob slots were derived from the layout
    /// this was invisible, because every knob binding went to 16/17/18.
    pub fn knob_slots_start_at_the_first_button(&self) -> bool {
        self.button_count() == 0 && self.layers.iter().any(|l| !l.knobs.is_empty())
    }

    /// Refuse a layout whose knob slots would land on top of its buttons.
    ///
    /// `rows`/`columns` place the knobs, and a layer that lists more buttons
    /// than the layout declares pushes real button bindings into the knob's
    /// slot range -- or, with `rows: 0`, puts the knob at key IDs 1/2/3,
    /// which on a device with three buttons ARE the buttons. Nothing
    /// downstream can detect that: both writes are well-formed and the
    /// device accepts both. The declared layout is the only place it can be
    /// caught, so it is caught here.
    pub fn check_slot_layout(&self) -> Result<()> {
        let declared = self.button_count();
        for (idx, layer) in self.layers.iter().enumerate() {
            let listed: usize = layer.buttons.iter().map(Vec::len).sum();
            if listed > declared {
                anyhow::bail!(
                    "Layer {} lists {} button(s) but the layout declares {} \
                     (rows {} x columns {}). The extra binding would be written \
                     to key ID {}, which is knob {} territory.",
                    idx,
                    listed,
                    declared,
                    self.rows,
                    self.columns,
                    declared + 1,
                    0
                );
            }
        }
        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file at {:?}", path.as_ref()))?;
        let config: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML syntax in {:?}", path.as_ref()))?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.layers.is_empty() {
            anyhow::bail!("Configuration must contain at least one layer.");
        }
        self.check_slot_layout()?;

        for (idx, layer) in self.layers.iter().enumerate() {
            for row in &layer.buttons {
                for key_str in row {
                    crate::protocol::Action::parse(key_str).with_context(|| {
                        format!("Invalid button action '{}' in layer {}", key_str, idx)
                    })?;
                }
            }

            for (k_idx, knob) in layer.knobs.iter().enumerate() {
                for (event, binding) in knob.bindings() {
                    binding.validate().with_context(|| {
                        format!("knob {} {} binding in layer {}", k_idx, event.as_str(), idx)
                    })?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
