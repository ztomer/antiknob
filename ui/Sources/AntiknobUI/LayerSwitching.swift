// LayerSwitching.swift — how you get from one layer to another.
//
// This was the first section of the Switching tab, one tab away from the
// layers it switches between, while that tab's other two sections were about
// flashing firmware and reading diagnostics. It is the only global setting
// the Layers pane needs, so it sits under the layers as their footer rather
// than in a tab named after it.

import SwiftUI

struct LayerSwitchingSection: View {
    @ObservedObject var store: ConfigStore

    var body: some View {
        Section {
            Toggle("Double-tap the knob to switch layers", isOn: Binding(
                get: { store.cfg.doubleTapEnabled },
                set: { store.cfg.doubleTapSwitch = $0 }
            ))

            LabeledContent("Next layer") {
                ChordRecorder(
                    chord: $store.cfg.layerHotkey,
                    requireModifiers: true,
                    clearable: true
                )
            }

            LabeledContent("Previous layer") {
                ChordRecorder(
                    chord: $store.cfg.layerHotkeyBack,
                    requireModifiers: true,
                    clearable: true
                )
            }
        } header: {
            Text("Switching Between Layers")
        } footer: {
            Text("With double-tap on, a single press waits "
               + "\(Int(store.cfg.tapWindow * 1000)) ms to see if a second one follows. "
               + "The shortcuts and the menu bar switch layers either way.")
        }
    }
}
