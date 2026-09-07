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
            PropertyGrid {
                StatusRow(
                    label: "Daemon Socket",
                    value: store.daemonConnected
                        ? "Connected (/tmp/antiknob.sock)"
                        : "Offline (Local fallback)"
                ) {
                    StatusDot(color: store.daemonConnected ? .green : .orange)
                }

                StatusRow(label: "Knob Hardware", value: store.hardwareProduct) {
                    StatusDot(color: store.hardwareConnected ? .green : .secondary)
                }

                StatusRow(
                    label: "Connection Mode",
                    value: store.hardwareConnected ? store.transportDisplay : "Not connected"
                ) {
                    Image(systemName: store.transportIcon)
                        .foregroundStyle(store.hardwareConnected
                                         ? store.transportColor : Color.secondary)
                        .frame(width: 16)
                }

                StatusRow(
                    label: "Power & Battery",
                    value: store.hardwareConnected ? store.powerDescription : "Disconnected"
                ) {
                    // `store.powerIcon`, not a local bolt/battery guess: this
                    // was the third copy of that mapping in the app.
                    Image(systemName: store.powerIcon)
                        .foregroundStyle(store.hardwareConnected
                                         ? Color.accentColor : Color.secondary)
                        .frame(width: 16)
                }

                if let msg = store.statusMessage {
                    StatusRow(label: "Last Operation", value: msg) { EmptyView() }
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
