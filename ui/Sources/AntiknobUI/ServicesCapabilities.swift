// ServicesCapabilities.swift — what the connected daemon can be asked to do.
//
// Split from ServicesPane for the type-body-length cap, along the seam
// between the pane's status rows and the one section that goes and asks the
// daemon a question.
//
// This section used to render a hand-written array of eleven `ApiToolInfo`
// values. The daemon exposes seventeen commands, all generated from one
// table that also generates its CLI. The hand-written copy advertised a
// `list_devices` tool that has never existed, gave `upload_keymap` a
// `yaml_content` parameter, `set_layer` a `layer` where the schema says
// `index`, and `set_led` a `color` its schema does not carry -- so anyone
// reading it to write a call would have written one the daemon refuses.

import SwiftUI

extension ServicesPane {
    // MARK: - Capabilities

    var capabilitiesSection: some View {
        Section {
            // A Button, not a DisclosureGroup.
            //
            // The group here never opened. Its label was a full-width HStack,
            // which leaves the hit area on the triangle glyph alone, and no
            // press on any part of it -- glyph, label, or row -- changed
            // `capabilitiesExpanded`. So the pane drew "Expand" beside a list
            // nobody could reach: the same defect as every other one this
            // sweep is about, an affordance that does nothing.
            //
            // A Button with an explicit `contentShape` has one hit area, the
            // whole row, and it can be verified.
            Button {
                withAnimation { capabilitiesExpanded.toggle() }
            } label: {
                HStack {
                    Image(systemName: capabilitiesExpanded ? "chevron.down" : "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .frame(width: 12)
                    Label(capabilitiesTitle, systemImage: "wrench.and.screwdriver")
                        .fontWeight(.medium)
                    Spacer()
                    if callable != nil {
                        Text(capabilitiesExpanded ? "Collapse" : "Expand")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(capabilitiesTitle)
            .accessibilityHint(capabilitiesExpanded ? "Collapse the list" : "Expand the list")

            if capabilitiesExpanded {
                capabilitiesBody
            }
        } footer: {
            Text(callable == nil
                 ? "Read from the daemon's own command table when it is reachable."
                 : "Read from the daemon's command table — the same one that generates "
                   + "its CLI and its MCP tool list, so this cannot drift from either.")
        }
    }

    private var callable: [DaemonCommand]? {
        commands.map(DaemonCommand.callable)
    }

    private var capabilitiesTitle: String {
        if let callable {
            return "Exposed Capabilities (\(callable.count) Tools)"
        }
        return commandsError == nil ? "Reading capabilities…" : "Capabilities unavailable"
    }

    @ViewBuilder
    private var capabilitiesBody: some View {
        if let callable {
            ForEach(callable) { tool in
                toolRow(tool)
                if tool.id != callable.last?.id {
                    Divider()
                }
            }
        } else if let commandsError {
            // Named rather than blank: an empty list and a daemon that could
            // not be asked are different facts.
            Text(commandsError)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private func toolRow(_ tool: DaemonCommand) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(tool.name)
                    .font(.system(.subheadline, design: .monospaced))
                    .fontWeight(.semibold)

                Spacer()

                if !tool.params.isEmpty {
                    Text(tool.params.map(\.signature).joined(separator: ", "))
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

            Text(tool.about)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            if !tool.cli.available, let reason = tool.cli.reason {
                // The registry records WHY a command is absent from a
                // surface. Carrying it distinguishes a considered omission
                // from one nobody has noticed.
                Text("Not on the CLI: \(reason)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 3)
    }

    func loadCommands() {
        commandsError = nil
        Task { @MainActor in
            do {
                commands = try await Task.detached(priority: .userInitiated) {
                    try SocketClient.shared.listCommands()
                }.value
            } catch {
                commands = nil
                commandsError = "Could not read the daemon's command table: "
                    + error.localizedDescription
            }
        }
    }
}
