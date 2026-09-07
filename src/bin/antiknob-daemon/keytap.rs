//! The global keyboard tap, straight onto CGEventTap.
//!
//! This replaced `rdev`, which reached the same API through `cocoa 0.22` and
//! `block 0.1.6` -- an unmaintained crate carrying a `static of uninhabited
//! type` that a future rustc turns into a hard error (rust-lang/rust#74840).
//! rdev 0.5.3 is its newest release, so there was nothing to bump to.
//!
//! The port is small because the daemon never wanted rdev's abstraction: both
//! call sites immediately converted `rdev::Key` back into a CG keycode
//! through a hand-written mirror table. A CGEventTap reports CG keycodes
//! natively, so that table is gone rather than ported.
//!
//! Two things this does better than the crate it replaces:
//!
//! * Modifier sides. Modifiers arrive as `FlagsChanged`, not KeyDown/KeyUp,
//!   and the event carries no up/down bit -- it must be derived from the
//!   flags. The device-dependent NX bits distinguish left from right, so
//!   releasing one Control while the other is held is reported for the key
//!   that actually moved (see `decode`).
//! * Failure that says what to do. A tap that cannot be created is almost
//!   always a missing Accessibility grant; that is now what the message says.

use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};

/// One physical key transition, in CG keycodes -- the only thing the daemon
/// ever wanted from an input library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: u16,
    pub pressed: bool,
}

/// What the tap should do with an event it was shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Hand it on to whoever is listening next.
    Pass,
    /// Consume it; nobody downstream sees it.
    Swallow,
}

/// Modifier keycodes, paired with the flag bits that say whether they are
/// down. The second column is the device-dependent (per-side) bit and the
/// third the device-independent one shared by both sides of that modifier.
///
/// `fn` has no per-side bit because there is only one of it.
const MODIFIERS: &[(u16, u64, u64)] = &[
    (59, 0x0000_0001, 0x0004_0000), // control, left
    (62, 0x0000_2000, 0x0004_0000), // control, right
    (56, 0x0000_0002, 0x0002_0000), // shift, left
    (60, 0x0000_0004, 0x0002_0000), // shift, right
    (55, 0x0000_0008, 0x0010_0000), // command, left
    (54, 0x0000_0010, 0x0010_0000), // command, right
    (58, 0x0000_0020, 0x0008_0000), // alt/option, left
    (61, 0x0000_0040, 0x0008_0000), // alt/option, right
    (63, 0x0000_0000, 0x0080_0000), // fn
];

/// Every per-side bit belonging to the same modifier as `code`.
fn side_bits_for(code: u16) -> u64 {
    let Some((_, _, family)) = MODIFIERS.iter().find(|(c, _, _)| *c == code) else {
        return 0;
    };
    MODIFIERS
        .iter()
        .filter(|(_, _, f)| f == family)
        .map(|(_, side, _)| side)
        .sum()
}

/// Turn a raw CG event into a key transition, or `None` if it is not one.
///
/// Pure, so the whole decode is testable without a tap or an Accessibility
/// grant -- which matters, because the tap itself cannot run in CI.
pub fn decode(event_type: CGEventType, code: u16, flags: u64) -> Option<KeyEvent> {
    match event_type {
        CGEventType::KeyDown => Some(KeyEvent {
            code,
            pressed: true,
        }),
        CGEventType::KeyUp => Some(KeyEvent {
            code,
            pressed: false,
        }),
        CGEventType::FlagsChanged => {
            let (_, side, family) = MODIFIERS.iter().find(|(c, _, _)| *c == code)?;
            // Prefer the per-side bit, so releasing one Control while the
            // other is held is reported for the key that moved. Some
            // keyboards and all synthetic events report only the shared bit,
            // and reading a press as a release there would wedge the held-
            // modifier set -- so fall back when no side bit is present at all.
            let pressed = if *side != 0 && flags & side_bits_for(code) != 0 {
                flags & side != 0
            } else {
                flags & family != 0
            };
            Some(KeyEvent { code, pressed })
        }
        _ => None,
    }
}

/// Watch the keyboard without ever consuming anything.
pub fn listen(mut on_key: impl FnMut(KeyEvent) + Send + 'static) -> Result<(), String> {
    run_tap(CGEventTapOptions::ListenOnly, move |ev| {
        on_key(ev);
        Decision::Pass
    })
}

/// Watch the keyboard and consume what the callback claims.
pub fn grab(on_key: impl FnMut(KeyEvent) -> Decision + Send + 'static) -> Result<(), String> {
    run_tap(CGEventTapOptions::Default, on_key)
}

/// Install the tap and pump this thread's run loop until it stops.
///
/// Returns only when the tap ends, which the caller treats as a reason to
/// rebuild it. A tap the system disables (it does that when a callback is too
/// slow, and on some user input) stops the loop deliberately rather than
/// sitting in it receiving nothing: silence is the one failure a tap cannot
/// report, so the supervisor is given something to react to.
fn run_tap(
    options: CGEventTapOptions,
    on_key: impl FnMut(KeyEvent) -> Decision + Send + 'static,
) -> Result<(), String> {
    let on_key = std::sync::Mutex::new(on_key);

    let result = CGEventTap::with_enabled(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        options,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
        ],
        move |_proxy, event_type, event| {
            if matches!(
                event_type,
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
            ) {
                CFRunLoop::get_current().stop();
                return CallbackResult::Keep;
            }

            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
            let Ok(code) = u16::try_from(code) else {
                return CallbackResult::Keep;
            };
            let Some(key) = decode(event_type, code, event.get_flags().bits()) else {
                return CallbackResult::Keep;
            };

            // A poisoned lock means the callback panicked once already;
            // passing the event through is the only safe answer left.
            let Ok(mut cb) = on_key.lock() else {
                return CallbackResult::Keep;
            };
            match cb(key) {
                Decision::Pass => CallbackResult::Keep,
                Decision::Swallow => CallbackResult::Drop,
            }
        },
        CFRunLoop::run_current,
    );

    result.map_err(|()| accessibility_hint())
}

/// The only realistic reason a tap fails to create, and the one the user can
/// act on. One definition: both `probe` and `run_tap` report it.
fn accessibility_hint() -> String {
    "could not create the event tap -- grant Accessibility (and Input Monitoring) \
     to this binary in System Settings > Privacy & Security"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const L_CTRL: u16 = 59;
    const R_CTRL: u16 = 62;
    const FN_KEY: u16 = 63;
    const CTRL_FLAG: u64 = 0x0004_0000;
    const L_CTRL_BIT: u64 = 0x0000_0001;
    const R_CTRL_BIT: u64 = 0x0000_2000;

    #[test]
    fn plain_keys_decode_by_event_type() {
        assert_eq!(
            decode(CGEventType::KeyDown, 40, 0),
            Some(KeyEvent {
                code: 40,
                pressed: true
            })
        );
        assert_eq!(
            decode(CGEventType::KeyUp, 40, 0),
            Some(KeyEvent {
                code: 40,
                pressed: false
            })
        );
    }

    #[test]
    fn events_that_are_not_key_transitions_are_ignored() {
        assert_eq!(decode(CGEventType::MouseMoved, 0, 0), None);
        assert_eq!(decode(CGEventType::ScrollWheel, 0, 0), None);
        // FlagsChanged for a key that is not a modifier is not a transition.
        assert_eq!(decode(CGEventType::FlagsChanged, 40, CTRL_FLAG), None);
    }

    #[test]
    fn a_modifier_press_and_release_are_derived_from_the_flags() {
        assert_eq!(
            decode(CGEventType::FlagsChanged, L_CTRL, CTRL_FLAG | L_CTRL_BIT),
            Some(KeyEvent {
                code: L_CTRL,
                pressed: true
            })
        );
        assert_eq!(
            decode(CGEventType::FlagsChanged, L_CTRL, 0),
            Some(KeyEvent {
                code: L_CTRL,
                pressed: false
            })
        );
    }

    /// The reason the per-side bits are read at all. Releasing left Control
    /// while right Control is still held leaves the shared Control flag SET,
    /// so a decoder that only looked at that bit would report the release as
    /// a press and leave "ctrl" stuck in the held set forever.
    #[test]
    fn releasing_one_side_while_the_other_is_held_is_reported_for_the_key_that_moved() {
        let flags = CTRL_FLAG | R_CTRL_BIT; // right still down, left just up
        assert_eq!(
            decode(CGEventType::FlagsChanged, L_CTRL, flags),
            Some(KeyEvent {
                code: L_CTRL,
                pressed: false
            })
        );
        assert_eq!(
            decode(CGEventType::FlagsChanged, R_CTRL, flags),
            Some(KeyEvent {
                code: R_CTRL,
                pressed: true
            })
        );
    }

    /// Synthetic events, and some keyboards, set only the shared bit. Reading
    /// that as a release would wedge the held-modifier set, so the shared bit
    /// is trusted when no side bit is present at all.
    #[test]
    fn a_press_reported_without_side_bits_is_still_a_press() {
        assert_eq!(
            decode(CGEventType::FlagsChanged, L_CTRL, CTRL_FLAG),
            Some(KeyEvent {
                code: L_CTRL,
                pressed: true
            })
        );
    }

    #[test]
    fn fn_has_no_side_bit_and_still_decodes() {
        assert_eq!(
            decode(CGEventType::FlagsChanged, FN_KEY, 0x0080_0000),
            Some(KeyEvent {
                code: FN_KEY,
                pressed: true
            })
        );
        assert_eq!(
            decode(CGEventType::FlagsChanged, FN_KEY, 0),
            Some(KeyEvent {
                code: FN_KEY,
                pressed: false
            })
        );
    }

    /// Every modifier this daemon tracks must decode, or a chord that
    /// includes it can never match.
    #[test]
    fn every_tracked_modifier_decodes_both_ways() {
        for (code, side, family) in MODIFIERS {
            let down = decode(CGEventType::FlagsChanged, *code, family | side);
            let up = decode(CGEventType::FlagsChanged, *code, 0);
            assert_eq!(
                down,
                Some(KeyEvent {
                    code: *code,
                    pressed: true
                }),
                "keycode {code} did not decode as a press"
            );
            assert_eq!(
                up,
                Some(KeyEvent {
                    code: *code,
                    pressed: false
                }),
                "keycode {code} did not decode as a release"
            );
        }
    }
}
