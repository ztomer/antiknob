use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnobConfig {
    #[serde(default)]
    pub ccw: Option<String>,
    #[serde(default)]
    pub press: Option<String>,
    #[serde(default)]
    pub cw: Option<String>,
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

impl DeviceConfig {
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

        for (idx, layer) in self.layers.iter().enumerate() {
            for row in &layer.buttons {
                for key_str in row {
                    crate::protocol::Action::parse(key_str).with_context(|| {
                        format!("Invalid button action '{}' in layer {}", key_str, idx)
                    })?;
                }
            }

            for (k_idx, knob) in layer.knobs.iter().enumerate() {
                if let Some(ref ccw) = knob.ccw {
                    crate::protocol::Action::parse(ccw).with_context(|| {
                        format!(
                            "Invalid knob {} CCW action '{}' in layer {}",
                            k_idx, ccw, idx
                        )
                    })?;
                }
                if let Some(ref press) = knob.press {
                    crate::protocol::Action::parse(press).with_context(|| {
                        format!(
                            "Invalid knob {} press action '{}' in layer {}",
                            k_idx, press, idx
                        )
                    })?;
                }
                if let Some(ref cw) = knob.cw {
                    crate::protocol::Action::parse(cw).with_context(|| {
                        format!("Invalid knob {} CW action '{}' in layer {}", k_idx, cw, idx)
                    })?;
                }
            }
        }

        Ok(())
    }
}
