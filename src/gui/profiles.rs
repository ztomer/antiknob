use crate::config::DeviceConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn export_profile_yaml(config: &DeviceConfig) -> Result<String> {
    serde_yaml::to_string(config).context("Failed to serialize configuration to YAML")
}

pub fn save_profile_file(config: &DeviceConfig, path: &Path) -> Result<()> {
    let yaml = export_profile_yaml(config)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directories")?;
    }
    fs::write(path, yaml).with_context(|| format!("Failed to write profile to {:?}", path))?;
    Ok(())
}

pub fn load_profile_file(path: &Path) -> Result<DeviceConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read profile file {:?}", path))?;
    let config: DeviceConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML profile from {:?}", path))?;
    Ok(config)
}
