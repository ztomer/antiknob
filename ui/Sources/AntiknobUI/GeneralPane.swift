// GeneralPane.swift — Global settings and daemon/hardware status.
// Configures double-tap switching, cycling hotkeys, and slot binding.

import SwiftUI

struct GeneralPane: View {
    @ObservedObject var store: ConfigStore

    var body: some View {
        Form {
            switchingSection
            slotBindingSection
            startupSection
            systemStatusSection
        }
        .formStyle(.grouped)
    }

    private var switchingSection: some View {
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
            Text("Layer Switching")
        } footer: {
            Text("Double-tap waits \(Int(store.cfg.tapWindow * 1000)) ms before a single "
               + "press acts. Turn it off for instant presses and switch layers with the "
               + "shortcuts or the menu bar instead. The shortcuts cycle through the "
               + "layers in a loop and require at least one modifier key.")
        }
    }

    private var slotBindingSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("Firmware Slot Translation")
                    .font(.subheadline)
                    .fontWeight(.medium)
                Text("Antiknob binds the knob's 5 firmware slots to ⌃⌥F16..F20 once over USB. "
                   + "All gestures are then translated cleanly host-side by the daemon.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack {
                    Button {
                        store.bindSlots()
                    } label: {
                        if store.isBindingSlots {
                            HStack(spacing: 6) {
                                ProgressView().controlSize(.small)
                                Text("Flashing slots…")
                            }
                        } else {
                            Label("Flash Slot Bindings to Hardware", systemImage: "bolt.fill")
                        }
                    }
                    .disabled(store.isBindingSlots)

                    Spacer()
                }
                .padding(.top, 4)
            }
            .padding(.vertical, 4)
        } header: {
            Text("Hardware Configuration")
        }
    }

    private var startupSection: some View {
        Section("System Startup") {
            Toggle("Start at Login", isOn: Binding(
                get: { store.startOnLogin },
                set: { store.toggleStartOnLogin(enabled: $0) }
            ))
            Text("""
                Automatically launches the Antiknob daemon at user login to maintain \
                knob gestures, layer switching, and lighting control.
                """)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var systemStatusSection: some View {
        Section("Status & Diagnostics") {
            LabeledContent("Daemon Socket") {
                HStack(spacing: 6) {
                    Circle()
                        .fill(store.daemonConnected ? Color.green : Color.orange)
                        .frame(width: 8, height: 8)
                    Text(store.daemonConnected ? "Connected (/tmp/antiknob.sock)" : "Offline (Local fallback)")
                        .foregroundStyle(.secondary)
                }
            }

            LabeledContent("Knob Hardware") {
                HStack(spacing: 6) {
                    Circle()
                        .fill(store.hardwareConnected ? Color.green : Color.secondary)
                        .frame(width: 8, height: 8)
                    Text(store.hardwareProduct)
                        .foregroundStyle(.secondary)
                }
            }

            LabeledContent("Connection Mode") {
                HStack(spacing: 6) {
                    Image(systemName: store.transportIcon)
                        .foregroundStyle(store.hardwareConnected ? store.transportColor : Color.secondary)
                    Text(store.hardwareConnected ? store.transportDisplay : "Not connected")
                        .foregroundStyle(.secondary)
                }
            }

            LabeledContent("Power & Battery") {
                HStack(spacing: 6) {
                    Image(systemName: store.hardwareConnected ? "bolt.fill" : "battery.0")
                        .foregroundStyle(store.hardwareConnected ? Color.accentColor : Color.secondary)
                    Text(store.hardwareConnected ? store.powerDescription : "Disconnected")
                        .foregroundStyle(.secondary)
                }
            }

            if let msg = store.statusMessage {
                LabeledContent("Last Operation") {
                    Text(msg)
                        .foregroundStyle(.secondary)
                }
            }

            HStack {
                Spacer()
                Button("Refresh Diagnostics") {
                    store.refreshStatus()
                }
                .buttonStyle(.link)
            }
        }
    }
}
