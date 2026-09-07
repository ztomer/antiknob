use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
        self.button_count() + self.knobs * 3
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(rows: usize, columns: usize, buttons: Vec<Vec<&str>>, knobs: usize) -> DeviceConfig {
        DeviceConfig {
            model: default_model(),
            orientation: default_orientation(),
            rows,
            columns,
            knobs,
            layers: vec![LayerConfig {
                buttons: buttons
                    .into_iter()
                    .map(|r| r.into_iter().map(str::to_string).collect())
                    .collect(),
                knobs: (0..knobs)
                    .map(|_| KnobConfig {
                        ccw: Some("volumedown".to_string()),
                        press: Some("mute".to_string()),
                        cw: Some("volumeup".to_string()),
                    })
                    .collect(),
                led: None,
            }],
        }
    }

    /// The layout is the only thing that says where knob slots land, so the
    /// count it yields is what `key_id_for_knob` is handed.
    fn temp_home(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("antiknob-cfg-{}-{}", std::process::id(), tag))
    }

    #[test]
    fn the_default_layout_path_sits_beside_the_daemons_own_state() {
        let p = default_device_config_path(Path::new("/Users/x"));
        assert_eq!(
            p,
            PathBuf::from("/Users/x/Library/Application Support/antiknob/config.yaml")
        );
    }

    #[test]
    fn a_missing_layout_is_seeded_and_an_existing_one_is_never_touched() {
        let home = temp_home("seed");
        let _ = fs::remove_dir_all(&home);

        let path = resolve_device_config_path(&home, None).expect("seed");
        assert_eq!(path, default_device_config_path(&home));
        assert!(path.exists(), "first use must write the starter layout");
        assert!(DeviceConfig::load_from_file(&path).is_ok());

        // The user's edits survive a second resolve, however unusual.
        fs::write(
            &path,
            "model: ch57x-1
rows: 0
columns: 0
knobs: 1
layers: []
",
        )
        .unwrap();
        let again = resolve_device_config_path(&home, None).expect("resolve");
        let body = fs::read_to_string(&again).unwrap();
        assert!(
            body.contains("layers: []"),
            "an existing config was overwritten"
        );

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn an_explicit_path_wins_and_seeds_nothing() {
        let home = temp_home("explicit");
        let _ = fs::remove_dir_all(&home);
        let given = PathBuf::from("/tmp/somewhere/else.yaml");
        assert_eq!(
            resolve_device_config_path(&home, Some(given.clone())).unwrap(),
            given
        );
        assert!(
            !default_device_config_path(&home).exists(),
            "an explicit path must not seed the default one"
        );
        let _ = fs::remove_dir_all(&home);
    }

    /// The compiled-in starter has to be a layout the tool accepts, or first
    /// use writes a file that immediately fails to load.
    #[test]
    fn the_packaged_starter_layout_is_valid() {
        let cfg: DeviceConfig = serde_yaml::from_str(STARTER_CONFIG).expect("starter parses");
        cfg.validate().expect("starter validates");
        assert_eq!(cfg.button_count(), 3);
    }

    /// The layer LEDs are the user's way of telling layers apart at a
    /// glance, so the starter has to actually carry them -- and carry the
    /// colours asked for, not whatever survived an edit.
    #[test]
    fn the_starter_layout_gives_the_first_two_layers_their_colours() {
        let cfg: DeviceConfig = serde_yaml::from_str(STARTER_CONFIG).expect("starter parses");
        assert_eq!(cfg.layers[0].led.as_deref(), Some("backlight red"));
        assert_eq!(cfg.layers[1].led.as_deref(), Some("backlight green"));

        // And they render to the packets the device expects: mode 1, then
        // the RGB triple.
        let red = crate::protocol::build_led_packet(0, cfg.layers[0].led.as_ref().unwrap())
            .expect("red packet");
        assert_eq!(&red[2..8], &[0xB0, 0x00, 0x01, 255, 0, 0]);
        let green = crate::protocol::build_led_packet(1, cfg.layers[1].led.as_ref().unwrap())
            .expect("green packet");
        assert_eq!(&green[2..8], &[0xB0, 0x01, 0x01, 0, 255, 0]);
    }

    /// The third layer is deliberately UNSET. The request was a
    /// multicoloured breathe, and this firmware's mapped modes are
    /// off/backlight/shock/shock2/press -- none of them a breathe. Writing
    /// a steady colour here and calling it done would be the same defect as
    /// a flash reporting success it never earned.
    #[test]
    fn the_third_layer_has_no_led_until_a_breathing_mode_is_identified() {
        let cfg: DeviceConfig = serde_yaml::from_str(STARTER_CONFIG).expect("starter parses");
        assert_eq!(
            cfg.layers[2].led, None,
            "layer 3's LED must stay unset until `led-probe` finds a breathe"
        );
    }

    #[test]
    fn the_button_count_comes_from_the_declared_grid() {
        // VK01: one row of three keys.
        assert_eq!(
            cfg(1, 3, vec![vec!["play", "prev", "next"]], 1).button_count(),
            3
        );
        // The 15-key macropad the old hardcoded base happened to fit.
        assert_eq!(cfg(3, 5, vec![], 1).button_count(), 15);
        assert_eq!(cfg(0, 0, vec![], 1).button_count(), 0);
    }

    /// A layer binding more buttons than the grid declares pushes the extra
    /// one into the knob's first slot, where it is accepted and ignored.
    #[test]
    fn a_layer_with_more_buttons_than_the_grid_is_refused() {
        let bad = cfg(1, 2, vec![vec!["play", "prev", "next"]], 1);
        let err = bad
            .validate()
            .expect_err("3 buttons in a 1x2 grid must be refused");
        let msg = err.to_string();
        assert!(msg.contains("lists 3"), "{msg}");
        assert!(msg.contains("key ID 3"), "{msg}");

        // The same bindings in the grid that fits are fine.
        assert!(cfg(1, 3, vec![vec!["play", "prev", "next"]], 1)
            .validate()
            .is_ok());
    }

    /// `rows: 0` puts knob slots at key IDs 1/2/3. Harmless with no keys,
    /// destructive with them -- the caller has to confirm which device it is.
    #[test]
    fn a_zero_button_layout_is_flagged_because_knob_slots_start_at_key_one() {
        assert!(cfg(0, 0, vec![], 1).knob_slots_start_at_the_first_button());
        // Buttons declared: knob slots start after them, no confirmation needed.
        assert!(!cfg(1, 3, vec![vec!["play", "prev", "next"]], 1)
            .knob_slots_start_at_the_first_button());
        // No knobs at all: nothing to collide.
        assert!(!cfg(0, 0, vec![], 0).knob_slots_start_at_the_first_button());
    }

    /// A zero-button layout that still lists buttons is self-contradictory
    /// and never valid, with or without the --knob-only confirmation.
    #[test]
    fn a_zero_button_layout_that_binds_buttons_is_always_refused() {
        let err = cfg(0, 0, vec![vec!["play"]], 1)
            .validate()
            .expect_err("binding buttons a layout does not declare must be refused");
        let msg = err.to_string();
        assert!(msg.contains("lists 1 button(s)"), "{msg}");
        assert!(msg.contains("declares 0"), "{msg}");
    }
}
