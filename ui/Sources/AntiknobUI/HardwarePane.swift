// HardwarePane.swift — Standalone Hardware & Firmware Flashing view.
// Manages slot chords, standalone on-chip keymap flashing, auxiliary buttons,
// and USB device hardware telemetry.

import AppKit
import SwiftUI

enum KeymapTemplate: String, CaseIterable, Identifiable {
    case media = "Media Controller"
    case mouseWheel = "Native Mouse Wheel"
    case zoom = "Meeting Controller (Zoom)"
    case browser = "Browser Navigation"
    case custom = "Custom YAML"

    var id: String { rawValue }

    var defaultYaml: String {
        switch self {
        case .media:
            return """
            layers:
              - name: Media
                buttons: []
                knobs:
                  - ccw: voldown
                    press: mute
                    cw: volup
                led: backlight green
            """
        case .mouseWheel:
            return """
            layers:
              - name: Wheel
                buttons: []
                knobs:
                  - ccw: wheeldown
                    press: mclick
                    cw: wheelup
                led: backlight cyan
            """
        case .zoom:
            return """
            layers:
              - name: Meeting
                buttons: []
                knobs:
                  - ccw: cmd+shift+a
                    press: opt+y
                    cw: cmd+shift+v
                led: backlight red
            """
        case .browser:
            return """
            layers:
              - name: Browser
                buttons: []
                knobs:
                  - ccw: cmd+leftbracket
                    press: cmd+r
                    cw: cmd+rightbracket
                led: backlight blue
            """
        case .custom:
            return """
            layers:
              - name: Custom
                buttons: []
                knobs:
                  - ccw: ctrl+alt+f16
                    press: ctrl+alt+f17
                    cw: ctrl+alt+f18
                led: backlight white
            """
        }
    }
}

struct HardwarePane: View {
    @ObservedObject var store: ConfigStore
    @State private var selectedTemplate: KeymapTemplate = .media
    @State private var yamlText: String = KeymapTemplate.media.defaultYaml
    @State private var targetLayer: Int = 0
    @State private var isFlashingKeymap: Bool = false
    @State private var flashStatus: String?
    @State private var flashSuccess: Bool = true
    @State private var selectedSlotLayer: Int = -1 // -1 = All layers

    var body: some View {
        Form {
            deviceInfoSection
            endpointSection
            slotBindingSection
            standaloneKeymapSection
            auxiliaryButtonsSection
        }
        .formStyle(.grouped)
    }

    private var deviceInfoSection: some View {
        Section("Hardware & Transport Details") {
            PropertyGrid {
                StatusRow(label: "Device Model", value: store.hardwareProduct) {
                    StatusDot(color: store.hardwareConnected ? .green : .secondary)
                }
                StatusRow(
                    label: "Active Transport",
                    value: store.hardwareConnected ? store.transportDisplay : "Not detected",
                    valueStyle: store.hardwareConnected ? .primary : .secondary
                ) {
                    Image(systemName: store.transportIcon)
                        .foregroundStyle(store.hardwareConnected
                                         ? store.transportColor : Color.secondary)
                        .frame(width: 16)
                }
                StatusRow(
                    label: "Power Supply",
                    value: store.hardwareConnected ? store.powerDescription : "Disconnected"
                ) {
                    Image(systemName: store.powerIcon)
                        .foregroundStyle(store.hardwareConnected
                                         ? Color.accentColor : Color.secondary)
                        .frame(width: 16)
                }
            }
        }
    }

    /// Endpoints as a four-column table: index, transport tag, device name,
    /// device path. Each is a different kind of thing, so each gets its own
    /// column and reads down its own straight edge.
    private var endpointSection: some View {
        Section("HID Endpoints") {
            if store.devices.isEmpty {
                Text("No active HID endpoints")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                PropertyGrid(horizontalSpacing: 14, verticalSpacing: 6) {
                    GridRow {
                        Text("#").gridColumnAlignment(.leading)
                        Text("Transport").gridColumnAlignment(.leading)
                        Text("Device").gridColumnAlignment(.leading)
                        Text("Path").gridColumnAlignment(.leading)
                    }
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .textCase(.uppercase)

                    ForEach(Array(store.devices.enumerated()), id: \.offset) { idx, dev in
                        GridRow {
                            Text("\(idx + 1)")
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                            TagPill(text: dev["transport"] as? String ?? "")
                            Text(dev["name"] as? String ?? "Unknown Device")
                                .font(.caption.weight(.medium))
                            Text(dev["path"] as? String ?? "")
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    private var slotBindingSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("""
                    Bind the knob's onboard slots to chords (⌃⌥F16..F20) so the host \
                    daemon translates all twists, clicks, and sequences cleanly.
                    """)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    Picker("Target Layer", selection: $selectedSlotLayer) {
                        Text("All Layers (0..2)").tag(-1)
                        Text("Layer 1 (0)").tag(0)
                        Text("Layer 2 (1)").tag(1)
                        Text("Layer 3 (2)").tag(2)
                    }
                    .frame(maxWidth: 220)

                    Spacer()

                    Button {
                        let l = selectedSlotLayer >= 0 ? selectedSlotLayer : nil
                        store.bindSlots(layer: l)
                    } label: {
                        if store.isBindingSlots {
                            HStack(spacing: 6) {
                                ProgressView().controlSize(.small)
                                Text("Flashing slots…")
                            }
                        } else {
                            Label("Flash Slot Bindings", systemImage: "bolt.fill")
                        }
                    }
                    .disabled(store.isBindingSlots || !store.hardwareConnected)
                }
                .padding(.top, 4)
            }
            .padding(.vertical, 4)
        } header: {
            Text("Host Translation Chords (Recommended)")
        } footer: {
            Text("""
                Flashing binds CCW=⌃⌥F16, Press=⌃⌥F17, CW=⌃⌥F18. Run once to prepare \
                hardware for daemon translation.
                """)
        }
    }

    private var standaloneKeymapSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("""
                    Flash standalone actions directly to on-chip EEPROM. The knob operates \
                    without any background app or daemon on any macOS, Windows, or Linux system.
                    """)
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    Picker("Template", selection: $selectedTemplate) {
                        ForEach(KeymapTemplate.allCases) { t in
                            Text(t.rawValue).tag(t)
                        }
                    }
                    .onChange(of: selectedTemplate) { _, newT in
                        yamlText = newT.defaultYaml
                    }
                    .frame(maxWidth: 240)

                    Spacer()

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
                }

                TextEditor(text: $yamlText)
                    .font(.system(.body, design: .monospaced))
                    .frame(minHeight: 140)
                    .padding(4)
                    .background(RoundedRectangle(cornerRadius: 6).fill(Color(nsColor: .textBackgroundColor)))
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.3), lineWidth: 1))

                if let msg = flashStatus {
                    HStack(spacing: 6) {
                        Image(systemName: flashSuccess ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(flashSuccess ? .green : .red)
                        Text(msg)
                            .font(.caption)
                            .foregroundStyle(flashSuccess ? Color.primary : Color.red)
                    }
                }
            }
            .padding(.vertical, 4)
        } header: {
            Text("Standalone On-Chip Keymap Flashing")
        } footer: {
            Text("""
                Directly writes 64-byte USB HID report packets (report ID 0x03) and sends \
                commit marker 0xFD 0xFE 0xFF.
                """)
        }
    }

    private var auxiliaryButtonsSection: some View {
        Section("Auxiliary Hardware Keypad Buttons") {
            VStack(alignment: .leading, spacing: 6) {
                Text("For hardware variants equipped with 1 or 3 mechanical buttons beside the knob:")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack(spacing: 16) {
                    VStack(alignment: .leading) {
                        Text("Button 1").fontWeight(.medium)
                        Text("key_id = 0x01").font(.caption2).monospaced().foregroundStyle(.secondary)
                    }
                    Divider().frame(height: 24)
                    VStack(alignment: .leading) {
                        Text("Button 2").fontWeight(.medium)
                        Text("key_id = 0x02").font(.caption2).monospaced().foregroundStyle(.secondary)
                    }
                    Divider().frame(height: 24)
                    VStack(alignment: .leading) {
                        Text("Button 3").fontWeight(.medium)
                        Text("key_id = 0x03").font(.caption2).monospaced().foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)

                Text("Configure buttons in the standalone YAML editor under the 'buttons' array: [[key1, key2, key3]].")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(.vertical, 2)
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
            case .failure(let err):
                flashSuccess = false
                flashStatus = err.localizedDescription
            }
        }
    }
}
