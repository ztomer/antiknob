//! Which mode the knob's firmware is actually in.
//!
//! The settings app edits host layers and draws them as the knob's
//! behaviour, but a host layer only ever fires if the firmware sends the
//! bound slot chords. Flash the knob standalone and those layers become
//! decoration: correct, saved, and unreachable. Nothing on screen said so,
//! which is the same defect as a daemon reporting a tap it never had --
//! an intention presented as a fact.
//!
//! The device can answer. Its slot table says whether the knob's gestures
//! carry slot chords or ordinary actions, so the app reads rather than
//! assumes. Pure: a table in, a mode out.

use super::verify::parse_record;
use crate::firmware::DEVICE_LAYERS;
use crate::firmware::FD_KIND_KEYBOARD as KIND_KEYBOARD;
use crate::protocol::{key_id_for_knob, KnobEvent};
use std::fmt::Write as _;

/// What one knob slot holds, as far as the mode is concerned.
enum Slot {
    /// Nothing runs from it -- no entries, or all zero.
    Empty,
    /// One of the chords `bind-slots` flashes.
    Chord,
    /// Some other action the OS handles directly.
    Other,
}

/// Read a slot out of a record the way the firmware stores it.
///
/// Keyboard and media records are `0xFD` chains, and that is also what
/// `bind-slots` writes and an `FA` read returns, so they are read as
/// chains. The previous reader took mods and keycode from bytes 11/12 of
/// the dead `0xFE` layout, found no chord in a chain, and reported every
/// flashed knob as Standalone. A `0xFE` record the firmware stored with no
/// entries (`len = 0`) is Empty: it holds bytes and runs nothing. Other
/// kinds (mouse) keep their own `0xFE` layout and are read as before.
fn read_slot(record: &[u8]) -> Option<(u8, u8, Slot)> {
    if let Some(chain) = crate::fd::decode_chain(record) {
        if chain.kind == KIND_KEYBOARD || chain.kind == crate::fd::Kind::Media as u8 {
            let slot = if chain.values.iter().all(|v| *v == 0) {
                Slot::Empty
            } else if chain.kind == KIND_KEYBOARD
                && crate::fd::chord_of(&chain.values)
                    .is_some_and(|c| crate::host::bind::slot_chords().contains(&c))
            {
                Slot::Chord
            } else {
                Slot::Other
            };
            return Some((chain.key_id, chain.layer, slot));
        }
    }
    let (addr, action) = parse_record(record)?;
    let slot = if action.media == 0 && action.code == 0 {
        Slot::Empty
    } else {
        Slot::Other
    };
    Some((addr.key_id, addr.layer, slot))
}

/// What the firmware will do when the knob is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobMode {
    /// Knob gestures send the bound slot chords, so host layers run.
    HostTranslate,
    /// Knob gestures send ordinary actions the OS handles directly. Host
    /// layers are configured but cannot fire.
    Standalone,
    /// The table holds nothing for the knob's slots -- no device attached,
    /// a read that returned nothing, or genuinely unbound gestures. Not a
    /// mode; an absence of evidence, and it must not be reported as either
    /// of the other two.
    Unknown,
}

impl KnobMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            KnobMode::HostTranslate => "host-translate",
            KnobMode::Standalone => "standalone",
            KnobMode::Unknown => "unknown",
        }
    }
}

/// What one DEVICE layer's knob does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerMode {
    /// 0-based, as `Arrangement` and the CLI's `--layer` count.
    pub layer: u8,
    pub mode: KnobMode,
    /// The knob's keys on this layer holding a binding that is NOT one of
    /// the chords `bind-slots` flashes (`host::bind::SLOT_SPECS`). The
    /// daemon does not hear these, whatever the layer's mode says.
    pub not_slot_chords: Vec<u8>,
}

/// Read the mode of EVERY device layer off a slot-table dump.
///
/// Per layer, because the firmware's layers are separate keymaps: a chord
/// on layer 2 says nothing about what layer 1's knob sends. The aggregate
/// `classify` used to be the only reading, and with layer 1 carrying
/// `ctrl-alt-f16..f20` -- chords the daemon does not listen for -- it still
/// said host-translate because layers 2 and 3 carried the right ones.
///
/// `button_count` places the knob's slots -- the same layout question that
/// decides where bindings are written, so a wrong count here misreads the
/// table exactly as it would misflash it. A count no slot table can hold
/// yields no ids and reads as Unknown; the flash paths refuse it outright.
#[must_use]
pub fn classify_layers(records: &[Vec<u8>], button_count: usize) -> Vec<LayerMode> {
    // All five gestures, not the first three: hold+twist occupies real
    // slots on this firmware, and a classifier that never looks at them
    // reports on two thirds of the knob while sounding certain about it.
    let knob_ids: Vec<u8> = KnobEvent::ALL
        .iter()
        .filter_map(|e| key_id_for_knob(button_count, 0, *e).ok())
        .collect();
    let slots: Vec<(u8, u8, Slot)> = records.iter().filter_map(|r| read_slot(r)).collect();

    (0..DEVICE_LAYERS)
        .map(|layer| {
            let mut saw_knob_slot = false;
            let mut saw_chord = false;
            let mut not_slot_chords = Vec::new();
            for (key_id, wire_layer, slot) in &slots {
                if *wire_layer != layer + 1 || !knob_ids.contains(key_id) {
                    continue;
                }
                match slot {
                    // An empty slot is not evidence of a mode either way.
                    Slot::Empty => {}
                    Slot::Chord => {
                        saw_knob_slot = true;
                        saw_chord = true;
                    }
                    Slot::Other => {
                        saw_knob_slot = true;
                        not_slot_chords.push(*key_id);
                    }
                }
            }
            not_slot_chords.sort_unstable();
            not_slot_chords.dedup();
            let mode = match (saw_knob_slot, saw_chord) {
                (false, _) => KnobMode::Unknown,
                (true, true) => KnobMode::HostTranslate,
                (true, false) => KnobMode::Standalone,
            };
            LayerMode {
                layer,
                mode,
                not_slot_chords,
            }
        })
        .collect()
}

/// The whole knob's mode: host-translate when ANY layer is, because then a
/// host layer can fire on it; standalone when some layer holds bindings and
/// none is host-translated; unknown when no layer shows anything. Which
/// layer is which is `classify_layers`, and every surface that reports the
/// mode reports that too.
#[must_use]
pub fn classify(records: &[Vec<u8>], button_count: usize) -> KnobMode {
    aggregate(&classify_layers(records, button_count))
}

/// Fold per-layer modes into the knob's mode (see `classify`).
#[must_use]
pub fn aggregate(layers: &[LayerMode]) -> KnobMode {
    if layers.iter().any(|l| l.mode == KnobMode::HostTranslate) {
        KnobMode::HostTranslate
    } else if layers.iter().any(|l| l.mode == KnobMode::Standalone) {
        KnobMode::Standalone
    } else {
        KnobMode::Unknown
    }
}

/// The device layers whose knob sends the slot chords, read off the device.
#[must_use]
pub fn host_translated_layers(layers: &[LayerMode]) -> Vec<u8> {
    layers
        .iter()
        .filter(|l| l.mode == KnobMode::HostTranslate)
        .map(|l| l.layer)
        .collect()
}

fn layer_list(layers: &[u8]) -> String {
    layers
        .iter()
        .map(|l| (l + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One paragraph for a person, read from the DEVICE: which layers the
/// daemon hears, which it does not, and every layer whose knob keys carry
/// something other than the slot chords, named.
///
/// `host.json`'s record of what `bind-slots` last flashed is not consulted:
/// it said "All 3 device layers send the slot chords" over a knob whose
/// layer 1 had been rebound since. States the fact, never a command to run,
/// for the same reason as `Arrangement::describe`.
pub fn describe_layers(layers: &[LayerMode]) -> String {
    let bound = host_translated_layers(layers);
    let standalone: Vec<u8> = layers
        .iter()
        .filter(|l| l.mode == KnobMode::Standalone)
        .map(|l| l.layer)
        .collect();
    let blank: Vec<u8> = layers
        .iter()
        .filter(|l| l.mode == KnobMode::Unknown)
        .map(|l| l.layer)
        .collect();
    let mut out = if bound.is_empty() || bound.len() == layers.len() {
        crate::host::device_binding::arrangement(&bound, DEVICE_LAYERS).describe()
    } else {
        let mut s = format!("Device layer(s) {} are host-translated", layer_list(&bound));
        if !standalone.is_empty() {
            let _ = write!(
                s,
                "; layer(s) {} run standalone and are unaffected by the daemon",
                layer_list(&standalone)
            );
        }
        if !blank.is_empty() {
            let _ = write!(
                s,
                "; layer(s) {} hold nothing on the knob's keys",
                layer_list(&blank)
            );
        }
        s.push('.');
        s
    };
    for l in layers.iter().filter(|l| !l.not_slot_chords.is_empty()) {
        let keys: Vec<String> = l.not_slot_chords.iter().map(u8::to_string).collect();
        let _ = write!(
            out,
            " Device layer {}: key(s) {} carry something other than the slot chords, \
             so the daemon does not hear them.",
            l.layer + 1,
            keys.join(", ")
        );
    }
    out
}

#[cfg(test)]
#[path = "mode_tests.rs"]
mod tests;
