//! Output plans: resolved engine actions to primitive synthetic ops.
//!
//! Pure translation only; the daemon binary performs the actual posts.
//! Slice 2a synthesizes keys, scroll, and brightness (as fn-flagged
//! F14/F15, the vk01-anticater fix: the symbolic brightness hotkeys only
//! match synthetic events carrying `maskSecondaryFn`). Aux media keys and
//! launch/open/quit plan correctly here and synthesize in slice 2b.

use super::engine::{FiredAction, SequenceFire};
use super::AuxKey;

// CGEventFlags bit values (mirrored so this module stays dependency-free).
pub const FLAG_CMD: u64 = 0x0010_0000;
pub const FLAG_ALT: u64 = 0x0008_0000;
pub const FLAG_CTRL: u64 = 0x0004_0000;
pub const FLAG_SHIFT: u64 = 0x0002_0000;
pub const FLAG_FN: u64 = 0x0080_0000;

pub const CG_F14: u16 = 107;
pub const CG_F15: u16 = 113;

// NX key codes for systemDefined subtype-8 media events (slice 2b).
pub const NX_SOUND_UP: u8 = 0;
pub const NX_SOUND_DOWN: u8 = 1;
pub const NX_MUTE: u8 = 7;
pub const NX_PLAY: u8 = 16;
pub const NX_NEXT: u8 = 17;
pub const NX_PREV: u8 = 18;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutOp {
    Key { code: u16, flags: u64, down: bool },
    SleepMs(u64),
    Scroll { lines: i32 },
    AuxNx { key: u8 },
    MouseClick { button: super::MouseButton },
    Launch { bundle_id: String },
    OpenUrl { url: String },
    OpenPath { path: String },
    Quit { bundle_id: String, force: bool },
}

/// Flag bits for canonical modifier names; unknown names are ignored
/// (vk01-anticater logs and ignores them at the engine boundary too).
pub fn flags_for_mods(mods: &[String]) -> u64 {
    let mut flags = 0u64;
    for m in mods {
        match super::tap::canonical_mod(m) {
            "cmd" => flags |= FLAG_CMD,
            "alt" => flags |= FLAG_ALT,
            "ctrl" => flags |= FLAG_CTRL,
            "shift" => flags |= FLAG_SHIFT,
            "fn" => flags |= FLAG_FN,
            _ => {}
        }
    }
    flags
}

fn key_press(code: u16, flags: u64) -> Vec<OutOp> {
    vec![
        OutOp::Key {
            code,
            flags,
            down: true,
        },
        OutOp::Key {
            code,
            flags,
            down: false,
        },
    ]
}

/// Translate one resolved action to primitive ops. Key chords post
/// down+up back-to-back; sequence delays are waits BEFORE their step.
pub fn plan(action: &FiredAction) -> Vec<OutOp> {
    match action {
        FiredAction::Scroll { lines } => vec![OutOp::Scroll { lines: *lines }],
        FiredAction::KeyChord { key, mods } => key_press(*key, flags_for_mods(mods)),
        FiredAction::Sequence(steps) => {
            let mut ops = Vec::new();
            for step in steps {
                plan_step(step, &mut ops);
            }
            ops
        }
        FiredAction::Aux(key) => match key {
            AuxKey::VolumeUp => vec![OutOp::AuxNx { key: NX_SOUND_UP }],
            AuxKey::VolumeDown => vec![OutOp::AuxNx { key: NX_SOUND_DOWN }],
            AuxKey::Mute => vec![OutOp::AuxNx { key: NX_MUTE }],
            AuxKey::PlayPause => vec![OutOp::AuxNx { key: NX_PLAY }],
            AuxKey::Next => vec![OutOp::AuxNx { key: NX_NEXT }],
            AuxKey::Previous => vec![OutOp::AuxNx { key: NX_PREV }],
            AuxKey::BrightnessUp => key_press(CG_F15, FLAG_FN),
            AuxKey::BrightnessDown => key_press(CG_F14, FLAG_FN),
        },
        FiredAction::MouseClick { button } => vec![OutOp::MouseClick { button: *button }],
        FiredAction::LaunchApp { bundle_id } => vec![OutOp::Launch {
            bundle_id: bundle_id.clone(),
        }],
        FiredAction::OpenUrl { url } => vec![OutOp::OpenUrl { url: url.clone() }],
        FiredAction::OpenPath { path } => vec![OutOp::OpenPath { path: path.clone() }],
        FiredAction::QuitApp { bundle_id, force } => vec![OutOp::Quit {
            bundle_id: bundle_id.clone(),
            force: *force,
        }],
    }
}

fn plan_step(step: &SequenceFire, ops: &mut Vec<OutOp>) {
    if let Some(delay) = step.delay_ms {
        ops.push(OutOp::SleepMs(delay));
    }
    if let Some(key) = step.key {
        ops.extend(key_press(key, flags_for_mods(&step.mods)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_flags_cover_all_modifiers() {
        let flags = flags_for_mods(
            &["cmd", "shift", "alt", "ctrl", "fn", "bogus"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            flags,
            FLAG_CMD | FLAG_SHIFT | FLAG_ALT | FLAG_CTRL | FLAG_FN
        );
    }

    #[test]
    fn brightness_posts_fn_flagged_function_keys() {
        assert_eq!(
            plan(&FiredAction::Aux(AuxKey::BrightnessUp)),
            vec![
                OutOp::Key {
                    code: CG_F15,
                    flags: FLAG_FN,
                    down: true
                },
                OutOp::Key {
                    code: CG_F15,
                    flags: FLAG_FN,
                    down: false
                },
            ]
        );
        let down = &plan(&FiredAction::Aux(AuxKey::BrightnessDown))[0];
        assert_eq!(
            down,
            &OutOp::Key {
                code: CG_F14,
                flags: FLAG_FN,
                down: true
            }
        );
    }

    #[test]
    fn media_maps_to_nx_codes() {
        assert_eq!(
            plan(&FiredAction::Aux(AuxKey::Mute)),
            vec![OutOp::AuxNx { key: NX_MUTE }]
        );
        assert_eq!(
            plan(&FiredAction::Aux(AuxKey::PlayPause)),
            vec![OutOp::AuxNx { key: NX_PLAY }]
        );
    }

    #[test]
    fn sequence_waits_precede_their_step() {
        let ops = plan(&FiredAction::Sequence(vec![
            SequenceFire {
                key: None,
                mods: vec![],
                delay_ms: Some(100),
            },
            SequenceFire {
                key: Some(8),
                mods: vec!["cmd".to_string()],
                delay_ms: Some(50),
            },
        ]));
        assert_eq!(
            ops,
            vec![
                OutOp::SleepMs(100),
                OutOp::SleepMs(50),
                OutOp::Key {
                    code: 8,
                    flags: FLAG_CMD,
                    down: true
                },
                OutOp::Key {
                    code: 8,
                    flags: FLAG_CMD,
                    down: false
                },
            ]
        );
    }

    #[test]
    fn mouse_click_passes_button_through() {
        use super::super::MouseButton;
        assert_eq!(
            plan(&FiredAction::MouseClick {
                button: MouseButton::Middle
            }),
            vec![OutOp::MouseClick {
                button: MouseButton::Middle
            }]
        );
    }

    #[test]
    fn scroll_passes_through_and_launch_plans() {
        assert_eq!(
            plan(&FiredAction::Scroll { lines: -3 }),
            vec![OutOp::Scroll { lines: -3 }]
        );
        assert!(matches!(
            plan(&FiredAction::LaunchApp {
                bundle_id: "com.apple.Calculator".to_string()
            })[0],
            OutOp::Launch { .. }
        ));
    }
}
