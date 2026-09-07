// ChordRecorder.swift — System Settings capsule-style shortcut recorder.
// Intercepts keyDown events while armed and displays macOS-native glyphs.

import AppKit
import SwiftUI

struct ChordRecorder: View {
    @Binding var chord: KeyChordSpec?
    var requireModifiers: Bool = false
    var clearable: Bool = false

    @State private var recording: Bool = false
    @State private var monitor: Any?

    var body: some View {
        HStack(spacing: 6) {
            Button(action: { recording ? stop() : start() }) {
                Text(recording ? "Type shortcut…" : (chord?.display ?? "Record Shortcut"))
                    .font(.system(.body, design: .rounded))
                    .frame(minWidth: 108)
            }
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .tint(recording ? Color.accentColor : nil)

            if clearable && chord != nil && !recording {
                Button {
                    chord = nil
                } label: {
                    Image(systemName: "xmark.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tertiary)
                .help("Remove shortcut")
            }
        }
        .onDisappear {
            stop()
        }
    }

    private func start() {
        recording = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            if event.keyCode == 53 { // Escape
                stop()
                return nil
            }

            var mods: [String] = []
            if event.modifierFlags.contains(.control) { mods.append("ctrl") }
            if event.modifierFlags.contains(.option) { mods.append("opt") }
            if event.modifierFlags.contains(.shift) { mods.append("shift") }
            if event.modifierFlags.contains(.command) { mods.append("cmd") }

            if requireModifiers && mods.isEmpty {
                NSSound.beep()
                return nil
            }

            // Reserve the knob's own firmware chords (ctrl+opt+F16..F20)
            if knobKeys.contains(Int64(event.keyCode)) &&
                mods.contains("ctrl") && mods.contains("opt") {
                NSSound.beep()
                return nil
            }

            chord = KeyChordSpec(key: event.keyCode, mods: mods, label: keyLabel(event))
            stop()
            return nil
        }
    }

    private func stop() {
        recording = false
        if let m = monitor {
            NSEvent.removeMonitor(m)
            monitor = nil
        }
    }
}
