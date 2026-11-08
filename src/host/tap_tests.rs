//! Tests for `tap`: modifier/key bookkeeping, the swallow policy, and the
//! AK.LEAK class -- no part of a slot chord leaves the grab tap.

use super::super::gesture::Gesture;
use super::super::ChordSpec;
use super::*;
use crate::host::hold_stats::HoldSample;

fn tap() -> TapEngine {
    TapEngine::new(HostConfig::default_config())
}

/// Hold the modifiers every slot chord carries. CG keycodes: control
/// 59, option 58, shift 56. Shift joined the set after `ctrl+alt+F18`
/// was found colliding with another app's global shortcut on real
/// hardware; this helper is the single place the tests say so.
fn slot_mods(t: &mut TapEngine) {
    t.key(59, true, 0);
    t.key(58, true, 0);
    t.key(56, true, 0);
}

#[test]
fn modifiers_tracked_and_released() {
    let mut t = tap();
    assert_eq!(t.key(59, true, 0), vec![]);
    assert_eq!(t.key(55, true, 0), vec![]);
    t.key(59, false, 0);
    // Twist-R with only cmd held: not a slot chord, passes through.
    assert_eq!(t.key(79, true, 10), vec![]);
    // And ctrl+alt WITHOUT shift is the chord this build moved off, so
    // it must pass through too -- otherwise the move bought nothing.
    let mut t2 = tap();
    t2.key(59, true, 0);
    t2.key(58, true, 0);
    assert_eq!(t2.key(79, true, 10), vec![]);
}

#[test]
fn autorepeat_presses_suppressed_until_release() {
    let mut t = tap();
    slot_mods(&mut t);
    let first = t.key(79, true, 0);
    assert_ne!(first.len(), 0);
    // Held-key repeats: no output.
    assert_eq!(t.key(79, true, 50), vec![]);
    assert_eq!(t.key(79, true, 100), vec![]);
    // Release re-arms: next press dispatches again.
    t.key(79, false, 150);
    assert_eq!(t.key(79, true, 200), first);
}

#[test]
fn slot_dispatch_end_to_end_through_tap() {
    let mut t = tap();
    slot_mods(&mut t);
    let out = t.key(106, true, 0);
    assert!(matches!(
        out[0],
        EngineEvent::Fire(super::super::engine::FiredAction::Scroll { lines: -3 })
    ));
}

#[test]
fn recorded_layer_hotkey_cycles_exact_match_only() {
    let mut cfg = HostConfig::default_config();
    cfg.layer_hotkey = Some(ChordSpec {
        key: 97,
        mods: vec!["cmd".to_string(), "opt".to_string()],
        label: "F6".to_string(),
    });
    let mut t = TapEngine::new(cfg);
    // Partial modifier set: no cycle (would hijack prefixes).
    t.key(55, true, 0);
    assert_eq!(t.key(97, true, 10), vec![]);
    t.key(97, false, 15);
    // Exact set (opt is alt): cycles forward.
    t.key(58, true, 20);
    assert_eq!(t.key(97, true, 30), vec![EngineEvent::LayerChanged(1)]);
}

#[test]
fn swallow_policy_consumed_press_and_release_only() {
    let mut t = tap();
    slot_mods(&mut t);
    // Consumed press -> release swallowed.
    assert_ne!(t.key(79, true, 0).len(), 0);
    assert!(t.release_swallow(79));
    // Second release: nothing to swallow.
    assert!(!t.release_swallow(79));
    // Pass-through press -> release passes through.
    assert_eq!(t.key(8, true, 10).len(), 0);
    assert!(!t.release_swallow(8));
    // Modifier releases never swallow (but do update mod state).
    assert!(!t.release_swallow(59));
    assert_eq!(t.key(79, true, 20).len(), 0);
}

#[test]
fn press_expiry_flows_through_poll() {
    let mut t = tap();
    slot_mods(&mut t);
    assert_eq!(t.key(64, true, 1000), vec![EngineEvent::NoOp]);
    assert_eq!(t.poll_expiry(1100), vec![]);
    let fired = t.poll_expiry(1300);
    assert!(matches!(fired[0], EngineEvent::Fire(_)));
}

// ---- AK.LEAK: what other applications see -------------------------

/// One key transition as the tap receives it: (code, pressed, ms).
type Ev = (u16, bool, u64);

/// The stream downstream of the grab tap, built only from `decide` and
/// `expire_held` verdicts -- the same two calls the daemon makes.
fn downstream(t: &mut TapEngine, input: &[Ev]) -> Vec<(u16, bool)> {
    let mut seen = Vec::new();
    for &(code, pressed, at) in input {
        for h in t.expire_held(at * 1000) {
            seen.push((h.code, true));
        }
        match t.decide(code, pressed, 0, at * 1000).verdict {
            Verdict::Pass => seen.push((code, pressed)),
            Verdict::Swallow | Verdict::Hold => {}
            Verdict::Replay(held) => {
                seen.extend(held.iter().map(|h| (h.code, true)));
                seen.push((code, pressed));
            }
        }
    }
    let end = input.last().map_or(0, |e| e.2) + 10_000;
    seen.extend(t.expire_held(end * 1000).iter().map(|h| (h.code, true)));
    seen
}

fn code_for_mod(name: &str) -> u16 {
    match name {
        "ctrl" => 59,
        "alt" => 58,
        "shift" => 56,
        other => panic!("slot chords carry no {other}"),
    }
}

/// The knob's chain: each modifier, the key, then the releases.
fn knob_detent(key: u16, mods: &[&str], at: u64, keys_first: bool) -> Vec<Ev> {
    let codes: Vec<u16> = mods.iter().map(|m| code_for_mod(m)).collect();
    let mut v: Vec<Ev> = codes.iter().map(|&c| (c, true, at)).collect();
    v.push((key, true, at + 1));
    let ups = codes.iter().rev().map(|&c| (c, false, at + 2));
    if keys_first {
        v.push((key, false, at + 2));
        v.extend(ups);
    } else {
        v.extend(ups);
        v.push((key, false, at + 2));
    }
    v
}

/// THE class test: no part of a slot chord -- key OR modifier, press OR
/// release -- leaves the tap, for every gesture and both release orders,
/// over repeated detents (two Option taps inside a double-tap window is
/// exactly what opened Claude's Quick Entry).
#[test]
fn a_slot_chord_never_leaves_the_tap() {
    for g in Gesture::ALL {
        let (key, mods) = super::super::gesture::slot_chord(g);
        for keys_first in [true, false] {
            let mut t = tap();
            let mut input = Vec::new();
            for detent in 0..4 {
                input.extend(knob_detent(key, &mods, detent * 120, keys_first));
            }
            let leaked = downstream(&mut t, &input);
            assert_eq!(leaked, vec![], "{g:?} (keys_first={keys_first}) leaked");
        }
    }
}

/// The other half of the contract: what a PERSON types still arrives,
/// complete and in the order it was typed, held or not.
#[test]
fn a_persons_keystrokes_arrive_whole_and_in_order() {
    let cases: Vec<Vec<Ev>> = vec![
        // A lone Control tap, quicker than the hold.
        vec![(59, true, 0), (59, false, 20)],
        // Control held past the hold, then released.
        vec![(59, true, 0), (59, false, 500)],
        // Ctrl-C typed fast: the C must not overtake the Control.
        vec![(59, true, 0), (8, true, 5), (8, false, 30), (59, false, 40)],
        // Double-tapped Option: still a double tap downstream.
        vec![
            (58, true, 0),
            (58, false, 30),
            (58, true, 90),
            (58, false, 120),
        ],
        // Shift-A, and Cmd-Shift-4 (cmd is never held).
        vec![
            (56, true, 0),
            (0, true, 10),
            (0, false, 20),
            (56, false, 25),
        ],
        vec![
            (55, true, 0),
            (56, true, 5),
            (21, true, 9),
            (21, false, 15),
            (56, false, 20),
            (55, false, 21),
        ],
        // ctrl-alt-F18 without shift is not a slot chord: it is not ours.
        vec![
            (59, true, 0),
            (58, true, 0),
            (79, true, 1),
            (79, false, 2),
            (58, false, 2),
            (59, false, 2),
        ],
    ];
    for input in cases {
        let mut t = tap();
        let want: Vec<(u16, bool)> = input.iter().map(|&(c, p, _)| (c, p)).collect();
        assert_eq!(downstream(&mut t, &input), want, "{input:?}");
    }
}

#[test]
fn a_held_press_expires_only_after_the_hold() {
    let mut t = tap();
    let hold_us = crate::policy::MOD_HOLD_MS * 1000;
    assert_eq!(t.decide(59, true, 0x4_0001, 100_000).verdict, Verdict::Hold);
    assert_eq!(t.expire_held(100_000 + hold_us - 1), vec![]);
    assert_eq!(
        t.expire_held(100_000 + hold_us),
        vec![Held {
            code: 59,
            flags: 0x4_0001,
            at_us: 100_000
        }]
    );
    // Replayed once: the release then passes on its own.
    assert_eq!(t.decide(59, false, 0, 900_000).verdict, Verdict::Pass);
}

// ---- AK.HOLDMEASURE: the instrument ---------------------------------

/// The hold instrument's whole contract in one chord: the gap is measured
/// from the FIRST modifier press (not the last, not the F-key's own
/// predecessor) to the F-key, in microseconds, tagged with the slot's
/// ordinal -- never the keycode.
#[test]
fn a_completed_slot_chord_records_the_gap_from_its_first_modifier() {
    let mut t = tap();
    t.decide(59, true, 0x4_0001, 1_000_000);
    t.decide(58, true, 0x4_0001, 1_000_300);
    t.decide(56, true, 0x4_0001, 1_000_900);
    assert_eq!(t.take_hold_samples(), vec![]);
    // F18 (79) is TwistR, third in `Gesture::ALL`.
    assert_eq!(t.decide(79, true, 0, 1_002_345).verdict, Verdict::Swallow);
    assert_eq!(
        t.take_hold_samples(),
        vec![HoldSample::Gap {
            slot: 2,
            gap_us: 2_345
        }]
    );
    // Drained: a second call has nothing.
    assert_eq!(t.take_hold_samples(), vec![]);
}

#[test]
fn an_expired_hold_is_counted_once() {
    let mut t = tap();
    t.decide(59, true, 0x4_0001, 100_000);
    t.expire_held(100_000 + crate::policy::MOD_HOLD_MS * 1000);
    assert_eq!(t.take_hold_samples(), vec![HoldSample::Expired]);
    // Nothing held now, so a later tick adds nothing.
    t.expire_held(900_000);
    assert_eq!(t.take_hold_samples(), vec![]);
}

/// A key a person typed is never sampled: a held modifier followed by an
/// ordinary key replays and records nothing, not even an empty gap.
#[test]
fn an_ordinary_key_after_a_held_modifier_records_nothing() {
    let mut t = tap();
    t.decide(59, true, 0x4_0001, 0);
    assert!(matches!(
        t.decide(8, true, 0, 5_000).verdict,
        Verdict::Replay(_)
    ));
    assert_eq!(t.take_hold_samples(), vec![]);
}

/// A consumed key that is not one of the five slot chords (a recorded layer
/// hotkey made of the same modifiers) is not the knob's chord, so it is not
/// a sample of the knob's timing.
#[test]
fn a_consumed_key_that_is_not_a_slot_chord_records_nothing() {
    let mut cfg = HostConfig::default_config();
    cfg.layer_hotkey = Some(ChordSpec {
        key: 97,
        mods: vec!["ctrl".to_string(), "opt".to_string(), "shift".to_string()],
        label: "F6".to_string(),
    });
    let mut t = TapEngine::new(cfg);
    t.decide(59, true, 0x4_0001, 0);
    t.decide(58, true, 0x4_0001, 100);
    t.decide(56, true, 0x4_0001, 200);
    assert_eq!(t.decide(97, true, 0, 900).verdict, Verdict::Swallow);
    assert_eq!(t.take_hold_samples(), vec![]);
}
