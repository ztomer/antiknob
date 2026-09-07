use crate::config::DeviceConfig;
use anyhow::{bail, Context, Result};
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

/// Write the six built-in presets as a daemon host config. Writes only
/// when the target is missing and never overwrites (the CLI importer owns
/// `--force`). Returns (layer count, warning count).
pub fn export_presets_to_host_file(path: &Path) -> Result<(usize, usize)> {
    use crate::host::migrate::migrate_all_presets;
    if path.exists() {
        bail!(
            "Host config exists: {} (delete it or use CLI --force)",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directories")?;
    }
    let (cfg, _leds, warnings) = migrate_all_presets();
    fs::write(path, cfg.to_json_pretty())
        .with_context(|| format!("Failed to write host config to {:?}", path))?;
    Ok((cfg.layers.len(), warnings.len()))
}
