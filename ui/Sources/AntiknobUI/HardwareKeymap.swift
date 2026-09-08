// HardwareKeymap.swift — the standalone flash: its templates and its pane.
//
// Split from HardwarePane for the type-body-length cap, along a real seam.
// The two halves of that pane answer opposite questions: the slot bindings
// make the knob talk to the daemon, and this makes it work without one.
// Flashing either REPLACES the other on the device, which is why this half
// says so in its own words rather than sharing a paragraph with the other.

import SwiftUI

/// A starting point for a standalone flash.
///
/// Every template declares its LAYOUT. It used to be omitted, which meant
/// zero buttons and a knob starting at key 1 -- wrong on the only hardware
/// this has ever been run against. And every LED line names a mode rather
/// than a colour, because the colour argument is not honoured here: mode 1
/// IS red and mode 2 IS green, whatever bytes follow.
enum KeymapTemplate: String, CaseIterable, Identifiable {
    case media = "Media Controller"
    case mouseWheel = "Native Mouse Wheel"
    case browser = "Browser Navigation"
    case custom = "Blank (Custom YAML)"

    var id: String { rawValue }

    /// The layout every template declares: one button, one knob. Measured on
    /// this VK01, where the five gestures occupy keys 2..6 and therefore
    /// leave room for exactly one button.
    private static let layout = """
        rows: 1
        columns: 1
        knobs: 1
        """

    var defaultYaml: String {
        switch self {
        case .media:
            return """
            \(Self.layout)
            layers:
              - led: red
                buttons:
                  - ["play"]
                knobs:
                  - ccw: volumedown
                    press: mute
                    cw: volumeup
                    hold_twist_l: prev
                    hold_twist_r: next
            """
        case .mouseWheel:
            return """
            \(Self.layout)
            layers:
              - led: green
                buttons:
                  - ["mclick"]
                knobs:
                  - ccw: wheeldown
                    press: click
                    cw: wheelup
                    hold_twist_l: cmd-minus
                    hold_twist_r: cmd-equal
            """
        case .browser:
            return """
            \(Self.layout)
            layers:
              - led: rainbow
                buttons:
                  - ["cmd-r"]
                knobs:
                  - ccw: wheelup
                    press: click
                    cw: wheeldown
                    hold_twist_l: cmd-leftbracket
                    hold_twist_r: cmd-rightbracket
            """
        case .custom:
            return """
            \(Self.layout)
            layers:
              - led: off
                buttons:
                  - []
                knobs:
                  - ccw:
                    press:
                    cw:
                    hold_twist_l:
                    hold_twist_r:
            """
        }
    }
}

extension HardwarePane {
    var standaloneKeymapSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("""
                    Runs with no daemon, on any machine. Overwrites the chords above, so the \
                    host layers stay quiet until you flash them again.
                    """)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                PropertyGrid {
                    GridRow {
                        Text("Template")
                            .foregroundStyle(.secondary)
                            .frame(width: Layout.controlLabel, alignment: .leading)
                            .gridColumnAlignment(.leading)
                        Dropdown(title: selectedTemplate.rawValue) {
                            Picker("", selection: $selectedTemplate) {
                                ForEach(KeymapTemplate.allCases) { t in
                                    Text(t.rawValue).tag(t)
                                }
                            }
                            .pickerStyle(.inline).labelsHidden()
                        }
                        .onChange(of: selectedTemplate) { _, newT in
                            yamlText = newT.defaultYaml
                        }
                        .gridColumnAlignment(.leading)

                        Button {
                            flashKeymap()
                        } label: {
                            if isFlashingKeymap {
                                HStack(spacing: 6) {
                                    ProgressView().controlSize(.small)
                                    Text("Writing to flash…")
                                }
                            } else {
                                Label("Flash Keymap to Hardware", systemImage: "arrow.up.doc.fill")
                            }
                        }
                        .disabled(isFlashingKeymap || !store.hardwareConnected)
                        .gridColumnAlignment(.leading)
                    }
                }

                TextEditor(text: $yamlText)
                    .font(.system(.body, design: .monospaced))
                    .frame(minHeight: 140)
                    .padding(4)
                    .background(RoundedRectangle(cornerRadius: 6)
                        .fill(Color(nsColor: .textBackgroundColor)))
                    .overlay(RoundedRectangle(cornerRadius: 6)
                        .stroke(Color.secondary.opacity(0.3), lineWidth: 1))

                if let msg = flashStatus {
                    HStack(spacing: 6) {
                        Image(systemName: flashSuccess
                              ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(flashSuccess ? .green : .red)
                        Text(msg)
                            .font(.caption)
                            .foregroundStyle(flashSuccess ? Color.primary : Color.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            .padding(.vertical, 4)
        } header: {
            Text("Standalone Keymap")
        } footer: {
            // Plain text: SwiftUI renders backticks literally, so a YAML key
            // written as code arrives on screen wearing punctuation.
            Text("""
                rows × columns is how many buttons come before the knob, which is what \
                places its five gestures. The led: line takes a mode — off, red, green, \
                ripple, rainbow, rgb — not a colour; each mode carries its own.
                """)
        }
    }

    private func flashKeymap() {
        isFlashingKeymap = true
        flashStatus = nil
        store.uploadKeymap(yaml: yamlText) { result in
            isFlashingKeymap = false
            switch result {
            case .success(let msg):
                flashSuccess = true
                flashStatus = "Success: \(msg)"
                // The knob's mode has just changed underneath the layer
                // panes; they must not keep drawing the old answer.
                store.refreshKnobMode()
            case .failure(let err):
                flashSuccess = false
                flashStatus = err.localizedDescription
            }
        }
    }
}
