// ServicesPane.swift — Services, Socket, MCP, and Daemon Capabilities view.
// Provides controls for Unix domain socket, MCP integration, event tap permissions,
// and lists all 9 exposed API tools (single source of truth).

import AppKit
import SwiftUI

struct ApiToolInfo: Identifiable {
    let id: String
    let name: String
    let description: String
    let params: [String]
}

let daemonTools: [ApiToolInfo] = [
    ApiToolInfo(
        id: "get_status",
        name: "get_status",
        description: "Query hardware detection, daemon socket status, and macOS event tap health.",
        params: []
    ),
    ApiToolInfo(
        id: "get_config",
        name: "get_config",
        description: "Read all configured host layers, actions, and double-tap switching toggle.",
        params: []
    ),
    ApiToolInfo(
        id: "set_config",
        name: "set_config",
        description: "Apply full runtime configuration and broadcast to the active tap engine.",
        params: ["config: object"]
    ),
    ApiToolInfo(
        id: "set_layer",
        name: "set_layer",
        description: "Switch the active physical knob layer (0-indexed).",
        params: ["layer: integer"]
    ),
    ApiToolInfo(
        id: "set_led",
        name: "set_led",
        description: "Configure RGB ring lighting mode and primary color hex code.",
        params: ["layer: integer", "mode: string", "color: string?"]
    ),
    ApiToolInfo(
        id: "bind_slots",
        name: "bind_slots",
        description: "Flash hardware slot chords (⌃⌥F16..F20) to knob onboard firmware.",
        params: ["layer: integer?", "dry_run: boolean?"]
    ),
    ApiToolInfo(
        id: "upload_keymap",
        name: "upload_keymap",
        description: "Flash custom keymap YAML directly to hardware onboard flash memory.",
        params: ["yaml_content: string"]
    ),
    ApiToolInfo(
        id: "list_apps",
        name: "list_apps",
        description: "Scan macOS system and user Applications for bundle IDs.",
        params: []
    ),
    ApiToolInfo(
        id: "list_devices",
        name: "list_devices",
        description: "Enumerate connected Anticater VK01 USB HID devices.",
        params: []
    ),
    ApiToolInfo(
        id: "read_slots",
        name: "read_slots",
        description: "Read slot table memory dump from hardware onboard flash memory.",
        params: ["group: integer?", "counters: [integer]?"]
    ),
    ApiToolInfo(
        id: "send_raw",
        name: "send_raw",
        description: "Send raw 64-byte HID report payload to device.",
        params: ["bytes: [string]"]
    )
]

struct ServicesPane: View {
    @ObservedObject var store: ConfigStore
    @State private var copiedClaude: Bool = false
    @State private var copiedAntigravity: Bool = false
    @State private var copiedToolId: String?
    @State private var capabilitiesExpanded: Bool = false

    var body: some View {
        Form {
            daemonSection
            eventTapSection
            mcpSection
            capabilitiesSection
        }
        .formStyle(.grouped)
    }

    private var daemonSection: some View {
        Section {
            LabeledContent("Daemon Socket") {
                HStack(spacing: 6) {
                    Circle()
                        .fill(store.daemonConnected ? Color.green : Color.orange)
                        .frame(width: 8, height: 8)
                    Text(store.daemonConnected ? "Connected" : "Offline")
                        .foregroundStyle(store.daemonConnected ? .primary : .secondary)
                }
            }

            LabeledContent("Socket Path") {
                Text("/tmp/antiknob.sock")
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
            }

            LabeledContent("Protocol") {
                Text("JSON-RPC 2.0 over AF_UNIX stream")
                    .foregroundStyle(.secondary)
            }

            HStack {
                Button {
                    store.refreshStatus()
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
            Text("Unix Domain Socket IPC")
        } footer: {
            Text("The daemon serves the single source of truth at /tmp/antiknob.sock. "
               + "The native UI, CLI, and third-party scripts communicate with this endpoint.")
        }
    }

    private var eventTapSection: some View {
        Section {
            LabeledContent("Event Tap Status") {
                HStack(spacing: 6) {
                    Circle()
                        .fill(store.tapActive ? Color.green : Color.orange)
                        .frame(width: 8, height: 8)
                    Text(store.tapActive ? "Active (Swallowing & Synthesizing)" : "Degraded (Permission Needed)")
                        .foregroundStyle(store.tapActive ? .primary : .secondary)
                }
            }

            if !store.tapActive {
                VStack(alignment: .leading, spacing: 8) {
                    Text("""
                        macOS requires Accessibility and Input Monitoring permissions to \
                        swallow and translate slot chords. Hardware USB flashing and LED \
                        control remain fully functional without this permission.
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
            Text("macOS Event Tap Interception")
        }
    }

    private var mcpSection: some View {
        Section {
            LabeledContent("Transport") {
                Text("stdio (Standard Input / Output)")
                    .foregroundStyle(.secondary)
            }

            LabeledContent("Launch Command") {
                Text("antiknob-daemon --mcp")
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 12) {
                Button {
                    copyClaudeConfig()
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: copiedClaude ? "checkmark" : "doc.on.doc")
                        Text(copiedClaude ? "Copied!" : "Copy Claude Desktop Config")
                    }
                }

                Button {
                    copyAntigravityConfig()
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
            Text("""
                Run antiknob-daemon with --mcp to connect LLM assistants directly to the \
                knob hardware and host configuration.
                """)
        }
    }

    private var capabilitiesSection: some View {
        Section {
            DisclosureGroup(isExpanded: $capabilitiesExpanded) {
                ForEach(daemonTools) { tool in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(tool.name)
                                .font(.system(.subheadline, design: .monospaced))
                                .fontWeight(.semibold)

                            Spacer()

                            if !tool.params.isEmpty {
                                Text(tool.params.joined(separator: ", "))
                                    .font(.caption2)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(Capsule().fill(.quaternary))
                                    .foregroundStyle(.secondary)
                            }

                            Button {
                                copyTool(tool.name)
                            } label: {
                                Image(systemName: copiedToolId == tool.name ? "checkmark" : "doc.on.doc")
                                    .font(.caption)
                            }
                            .buttonStyle(.borderless)
                            .help("Copy tool name")
                        }

                        Text(tool.description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 3)
                    if tool.id != daemonTools.last?.id {
                        Divider()
                    }
                }
            } label: {
                HStack {
                    Label("Exposed Capabilities (\(daemonTools.count) Tools)", systemImage: "wrench.and.screwdriver")
                        .fontWeight(.medium)
                    Spacer()
                    Text(capabilitiesExpanded ? "Collapse" : "Expand")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        } footer: {
            Text("All tools share identical schemas across Unix Domain Socket and MCP stdio interfaces.")
        }
    }

    private func copyClaudeConfig() {
        let json = """
        {
          "mcpServers": {
            "antiknob": {
              "command": "/Applications/Antiknob/bin/antiknob-daemon",
              "args": ["--mcp"]
            }
          }
        }
        """
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(json, forType: .string)
        withAnimation { copiedClaude = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            withAnimation { copiedClaude = false }
        }
    }

    private func copyAntigravityConfig() {
        let json = """
        {
          "antiknob": {
            "command": "/Applications/Antiknob/bin/antiknob-daemon",
            "args": ["--mcp"]
          }
        }
        """
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(json, forType: .string)
        withAnimation { copiedAntigravity = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            withAnimation { copiedAntigravity = false }
        }
    }

    private func copyTool(_ name: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(name, forType: .string)
        withAnimation { copiedToolId = name }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
            withAnimation { copiedToolId = nil }
        }
    }
}
