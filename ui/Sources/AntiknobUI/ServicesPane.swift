// ServicesPane.swift — the socket, the event tap, the MCP server, and what
// the daemon can actually be asked to do.
//
// The capability list here used to be a hand-written array of eleven tools
// with hand-written descriptions and parameters. The daemon exposes
// seventeen commands, generated from one table that also generates its CLI.
// The hand-written copy had drifted exactly as a second copy does: it
// advertised a `list_devices` tool that does not exist, gave `upload_keymap`
// a `yaml_content` parameter, `set_layer` a `layer` where the schema says
// `index`, and `set_led` a `color` its schema does not carry. It is read
// from the daemon now, so what the pane shows is what the connected build
// answers to.

import AppKit
import SwiftUI

struct ServicesPane: View {
    @ObservedObject var store: ConfigStore
    @State private var copiedClaude: Bool = false
    @State private var copiedAntigravity: Bool = false
    // Module-internal rather than private: the capabilities half of this
    // pane is an extension in ServicesCapabilities.swift, and an extension
    // in another file cannot see `private` members.
    @State var copiedToolId: String?
    @State var capabilitiesExpanded: Bool = false

    /// nil until the daemon has been asked. Empty is a different state from
    /// unread, and both are different from a list.
    @State var commands: [DaemonCommand]?
    @State var commandsError: String?

    var body: some View {
        Form {
            daemonSection
            eventTapSection
            mcpSection
            capabilitiesSection
        }
        .formStyle(.grouped)
        .onAppear { loadCommands() }
        .onChange(of: store.daemonConnected) { _, connected in
            if connected && commands == nil { loadCommands() }
        }
    }

    // MARK: - Socket

    private var daemonSection: some View {
        Section {
            PropertyGrid {
                StatusRow(
                    label: "Daemon Socket",
                    value: store.daemonConnected ? "Connected" : "Offline",
                    valueStyle: store.daemonConnected ? .primary : .secondary
                ) {
                    StatusDot(color: store.daemonConnected ? .green : .orange)
                }

                GridRow {
                    Text("Socket Path")
                        .foregroundStyle(.secondary)
                        .gridColumnAlignment(.leading)
                    // The socket in use, not the one the app hopes is there.
                    // /tmp/antiknob.sock is a convenience symlink the daemon
                    // creates when it can, and this pane printed it as fact
                    // on machines where it does not exist.
                    Text(store.socketPath ?? "Not connected")
                        .font(.system(.body, design: .monospaced))
                        .foregroundStyle(store.socketPath == nil ? .secondary : .primary)
                        .textSelection(.enabled)
                        .gridColumnAlignment(.leading)
                    EmptyView()
                }

                GridRow {
                    Text("Protocol")
                        .foregroundStyle(.secondary)
                        .gridColumnAlignment(.leading)
                    Text("JSON-RPC 2.0 over AF_UNIX stream")
                        .gridColumnAlignment(.leading)
                    EmptyView()
                }
            }

            HStack {
                Button {
                    store.refreshStatus()
                    loadCommands()
                } label: {
                    Label("Ping Daemon", systemImage: "arrow.clockwise")
                }

                Button {
                    store.restartDaemon()
                } label: {
                    Label("Restart Daemon", systemImage: "arrow.counterclockwise.circle")
                }

                Spacer()
            }
            .padding(.top, 4)
        } header: {
            Text("Daemon")
        } footer: {
            Text("This app, the CLI and any script reach the knob through the daemon.")
        }
    }

    // MARK: - Event tap

    private var eventTapSection: some View {
        Section {
            PropertyGrid {
                StatusRow(
                    label: "Event Tap Status",
                    value: store.tapActive
                        ? "Running"
                        : "Needs permission",
                    valueStyle: store.tapActive ? .primary : .secondary
                ) {
                    StatusDot(color: store.tapActive ? .green : .orange)
                }
            }

            if !store.tapActive {
                VStack(alignment: .leading, spacing: 8) {
                    Text("""
                        Antiknob needs Accessibility and Input Monitoring to intercept the \
                        knob's chords. Flashing and lighting still work without it; the host \
                        layers do not.
                        """)
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    if let err = store.tapError {
                        Text("Reason: \(err)")
                            .font(.caption2)
                            .foregroundStyle(.red)
                    }

                    Button {
                        store.openAccessibilitySettings()
                    } label: {
                        Label("Open Accessibility in System Settings", systemImage: "lock.shield")
                    }
                }
                .padding(.vertical, 4)
            }
        } header: {
            Text("Keyboard Interception")
        }
    }

    // MARK: - MCP

    private var mcpSection: some View {
        Section {
            PropertyGrid {
                PropertyRow(label: "Transport") {
                    Text("stdio (Standard Input / Output)")
                }

                PropertyRow(label: "Launch Command") {
                    // The resolved path, so the copied config points at a
                    // binary that is actually there. It was hardcoded to
                    // /Applications/Antiknob/bin, which is one of four
                    // places the daemon can be installed.
                    Text("\(store.daemonBinaryPath) --mcp")
                        .font(.system(.body, design: .monospaced))
                        .textSelection(.enabled)
                }
            }

            HStack(spacing: 12) {
                Button {
                    copy(Self.claudeConfig(daemon: store.daemonBinaryPath)) { copiedClaude = $0 }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: copiedClaude ? "checkmark" : "doc.on.doc")
                        Text(copiedClaude ? "Copied!" : "Copy Claude Desktop Config")
                    }
                }

                Button {
                    copy(Self.antigravityConfig(daemon: store.daemonBinaryPath)) {
                        copiedAntigravity = $0
                    }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: copiedAntigravity ? "checkmark" : "doc.on.doc")
                        Text(copiedAntigravity ? "Copied!" : "Copy Antigravity Config")
                    }
                }

                Spacer()
            }
            .padding(.top, 4)
        } header: {
            Text("Model Context Protocol (MCP) Server")
        } footer: {
            Text("Lets an AI assistant read the knob's status and change its configuration.")
        }
    }

    // MARK: - Actions

    static func claudeConfig(daemon: String) -> String {
        """
        {
          "mcpServers": {
            "antiknob": {
              "command": "\(daemon)",
              "args": ["--mcp"]
            }
          }
        }
        """
    }

    static func antigravityConfig(daemon: String) -> String {
        """
        {
          "antiknob": {
            "command": "\(daemon)",
            "args": ["--mcp"]
          }
        }
        """
    }

    private func copy(_ text: String, mark: @escaping (Bool) -> Void) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        withAnimation { mark(true) }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            withAnimation { mark(false) }
        }
    }

    func copyTool(_ name: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(name, forType: .string)
        withAnimation { copiedToolId = name }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
            withAnimation { copiedToolId = nil }
        }
    }
}
