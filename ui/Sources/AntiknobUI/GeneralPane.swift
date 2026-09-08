// GeneralPane.swift — Global settings and daemon/hardware status.
// Configures double-tap switching, cycling hotkeys, and slot binding.

import SwiftUI

struct GeneralPane: View {
    @ObservedObject var store: ConfigStore

    var body: some View {
        Form {
            startupSection
            systemStatusSection
        }
        .formStyle(.grouped)
    }

    private var startupSection: some View {
        Section("System Startup") {
            Toggle("Start at Login", isOn: Binding(
                get: { store.startOnLogin },
                set: { store.toggleStartOnLogin(enabled: $0) }
            ))
            Text("Starts the Antiknob daemon when you log in. Without it the knob's "
               + "gestures reach macOS unchanged.")
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
                        : "Not running — changes are saved but not applied"
                ) {
                    StatusDot(color: store.daemonConnected ? .green : .orange)
                }

                StatusRow(label: "Device", value: store.hardwareProduct) {
                    StatusDot(color: store.hardwareConnected ? .green : .secondary)
                }

                StatusRow(
                    label: "Connection",
                    value: store.hardwareConnected ? store.transportDisplay : "Not connected"
                ) {
                    Image(systemName: store.transportIcon)
                        .foregroundStyle(store.hardwareConnected
                                         ? store.transportColor : Color.secondary)
                        .frame(width: 16)
                }

                StatusRow(
                    label: "Power",
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
