// GeneralPane.swift — Global settings and daemon/hardware status.
// Configures double-tap switching, cycling hotkeys, and slot binding.

import SwiftUI

struct GeneralPane: View {
    @ObservedObject var store: ConfigStore

    var body: some View {
        Form {
            slotBindingSection
            startupSection
            systemStatusSection
        }
        .formStyle(.grouped)
    }

    private var slotBindingSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                // The old line named the mechanism -- "binds the knob's five
                // gesture slots to ⌃⌥F16..F20" -- to a reader who wanted to
                // know what the button was for. The chords are still spelled
                // out, on the Hardware pane, next to the control that picks
                // which device layer gets them.
                Text("Out of the box the knob talks straight to macOS, so nothing you "
                   + "configure here can run. Flashing it once, over USB, hands its "
                   + "gestures to Antiknob instead.")
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
                // Which DEVICE layer carries the chords -- shown only when
                // one does. With nothing bound, this line said "no host
                // layer can fire", which is what the warning directly above
                // it already says; two sentences for one fact, the second
                // one longer.
                if store.modePresentation.hostLayersCanFire,
                   let arrangement = store.deviceBindingSummary {
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
                            Label("Flash the Knob", systemImage: "bolt.fill")
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
            Text("Knob Control")
        }
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
