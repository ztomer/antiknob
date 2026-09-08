// LayerFlashRow.swift — whether the knob is sending its gestures here yet.
//
// This was a warning banner: an orange triangle and "These layers are
// inactive — the knob is flashed standalone." It is not a fault. A knob out
// of the box talks to macOS; flashing it once is a step in setting it up,
// like granting Accessibility. Dressing a setup step as a problem makes a
// working app look broken on first run.
//
// So it is a status row with the action beside it, and the same row after
// the flash reads as the ordinary state it is.
//
// One honest correction to the label: this is NOT per layer. `bind_slots`
// writes the slot chords into the knob's FIRMWARE, once, and every host
// layer works from then on -- there is no such thing as flashing one layer
// and not another. "Flash Layer to Device" would tell someone with five
// layers to do this five times.

import SwiftUI

struct LayerFlashRow: View {
    @ObservedObject var store: ConfigStore

    private var mode: ModePresentation { store.modePresentation }

    var body: some View {
        GridRow {
            Text("Knob")
                .foregroundStyle(.secondary)
                .gridColumnAlignment(.leading)

            Text(mode.statusText)
                .foregroundStyle(mode.hostLayersCanFire ? Color.primary : Color.secondary)
                .gridColumnAlignment(.leading)

            Button {
                store.bindSlots()
            } label: {
                if store.isBindingSlots {
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.small)
                        Text("Flashing…")
                    }
                } else {
                    Label(mode.flashActionTitle, systemImage: "bolt.fill")
                }
            }
            .disabled(store.isBindingSlots || !store.hardwareConnected)
            .help(store.hardwareConnected
                  ? "Writes ⌃⌥F16..F20 into the knob so Antiknob receives its gestures"
                  : "No knob detected")
            .gridColumnAlignment(.leading)
        }
    }
}
