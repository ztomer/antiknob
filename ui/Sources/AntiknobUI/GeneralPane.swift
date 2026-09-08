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
                Text("Antiknob binds the knob's 5 gesture slots to ⌃⌥F16..F20 once over USB. "
                   + "All five gestures are then translated host-side by the daemon.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                // Whether the flash has actually taken, said here rather
                // than only on every layer tab. `bind-slots` wrote three of
                // the five chords until this session, so a knob could be
                // bound and still have two gestures that reached nothing.
                if let banner = store.modePresentation.banner {
                    Label(banner, systemImage: store.modePresentation.icon)
                        .font(.caption)
                        .foregroundStyle(store.modePresentation.mode == .standalone
                                         ? Color.orange : Color.secondary)
                        .help(store.modePresentation.detail ?? banner)
                }
                if let arrangement = store.deviceBindingSummary {
                    // Which DEVICE layer carries the chords. It used to be
                    // repeated at the top of every host layer's tab, where
                    // it is not a fact about any one of them.
                    Text(arrangement)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }

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
                    .disabled(store.isBindingSlots || !store.hardwareConnected)
                    .help(store.hardwareConnected ? "" : "No knob detected")

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
                    // The socket in use. This printed `/tmp/antiknob.sock`
                    // as a constant; that path is a symlink the daemon
                    // creates when it can, and on a machine without it every
                    // call was going somewhere else entirely.
                    label: "Daemon Socket",
                    value: store.daemonConnected
                        ? "Connected (\(store.socketPath ?? "path unknown"))"
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
