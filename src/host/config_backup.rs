//! A way back from the last write to `host.json`.
//!
//! Every keystroke in the settings app is a `set_config`, and `set_config`
//! truncated the file and wrote over it. That is fine while the writer is
//! right and total while it is not: a UI bug cost a working two-layer setup
//! on 2026-09-08 -- the app crashed mid-delete, wrote a config with one
//! blank layer, and there was nothing on disk to go back to. Nothing had
//! gone wrong with the FILE. Everything had gone wrong with the thing
//! writing it, which is the case a backup exists for.
//!
//! Cheap enough to be unconditional: the file is under a kilobyte, and a
//! rotation of a handful of them costs less than one screenshot.

use crate::policy::BACKUP_KEEP_COUNT as KEEP;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// The rotation slot paths, newest first: `host.json.1` .. `host.json.N`.
/// Keeps `KEEP` previous versions (policy map): enough to survive a run of
/// bad writes -- the damage that prompted this was three writes deep by
/// the time anyone looked -- and few enough that the directory stays
/// readable.
pub fn slots(config_path: &Path) -> Vec<PathBuf> {
    (1..=KEEP)
        .map(|i| {
            let mut name = config_path.as_os_str().to_os_string();
            name.push(format!(".{i}"));
            PathBuf::from(name)
        })
        .collect()
}

/// Rotate the current config into slot 1, ageing the rest.
///
/// Called BEFORE a write, so slot 1 is always the version the write is about
/// to replace. Never fails the write it protects: a backup that cannot be
/// taken is a reason to log, not a reason to refuse to save the user's
/// work.
pub fn rotate(config_path: &Path) {
    if !config_path.exists() {
        return;
    }
    let slots = slots(config_path);
    // Oldest first, so nothing is overwritten before it has been moved on.
    for i in (1..slots.len()).rev() {
        let _ = std::fs::rename(&slots[i - 1], &slots[i]);
    }
    if let Err(e) = std::fs::copy(config_path, &slots[0]) {
        eprintln!("[ Wrn ] Could not back up {}: {e}", config_path.display());
    }
}

/// Write the config, keeping the version it replaces.
///
/// Written to a temporary file and renamed, so a process that dies mid-write
/// leaves the old file intact rather than a truncated one. `fs::write`
/// truncates first, which is a window where the config is half a file.
pub fn write_with_backup(config_path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    rotate(config_path);

    let mut tmp = config_path.as_os_str().to_os_string();
    tmp.push(".new");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, config_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("antiknob_backup_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("temp dir");
        d
    }

    #[test]
    fn the_replaced_version_is_kept() {
        let dir = tmpdir("keep");
        let cfg = dir.join("host.json");
        write_with_backup(&cfg, "first").expect("write");
        write_with_backup(&cfg, "second").expect("write");

        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "second");
        assert_eq!(
            std::fs::read_to_string(&slots(&cfg)[0]).unwrap(),
            "first",
            "slot 1 must hold what the last write replaced"
        );
    }

    /// The damage this exists for was several writes deep before anyone
    /// looked, so one slot would not have been enough.
    #[test]
    fn older_versions_age_through_the_slots() {
        let dir = tmpdir("age");
        let cfg = dir.join("host.json");
        for n in 0..4 {
            write_with_backup(&cfg, &format!("v{n}")).expect("write");
        }
        let slots = slots(&cfg);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "v3");
        assert_eq!(std::fs::read_to_string(&slots[0]).unwrap(), "v2");
        assert_eq!(std::fs::read_to_string(&slots[1]).unwrap(), "v1");
        assert_eq!(std::fs::read_to_string(&slots[2]).unwrap(), "v0");
    }

    /// Only `KEEP` versions are kept, or the directory grows without bound
    /// on a config the app rewrites on every keystroke.
    #[test]
    fn the_rotation_is_bounded() {
        let dir = tmpdir("bound");
        let cfg = dir.join("host.json");
        for n in 0..(KEEP + 6) {
            write_with_backup(&cfg, &format!("v{n}")).expect("write");
        }
        let present = slots(&cfg).iter().filter(|p| p.exists()).count();
        assert_eq!(present, KEEP);
        let extra = dir.join(format!("host.json.{}", KEEP + 1));
        assert!(!extra.exists(), "rotation kept more than {KEEP}");
    }

    /// A first write has nothing to back up, and must not fail trying.
    #[test]
    fn the_first_write_has_nothing_to_keep() {
        let dir = tmpdir("first");
        let cfg = dir.join("host.json");
        write_with_backup(&cfg, "only").expect("write");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "only");
        assert!(!slots(&cfg)[0].exists());
    }

    /// The write is a rename over a complete temporary file, so a process
    /// that dies partway leaves the old config whole rather than truncated.
    #[test]
    fn no_temporary_file_survives_a_successful_write() {
        let dir = tmpdir("tmp");
        let cfg = dir.join("host.json");
        write_with_backup(&cfg, "done").expect("write");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".new"))
            .collect();
        assert!(leftovers.is_empty(), "left a temp file: {leftovers:?}");
    }
}
