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
        }
    }

    private var callable: [DaemonCommand]? {
        commands.map(DaemonCommand.callable)
    }

    private var capabilitiesTitle: String {
        if let callable {
            return "Commands (\(callable.count))"
        }
        return commandsError == nil ? "Reading commands…" : "Commands unavailable"
    }

    @ViewBuilder
    private var capabilitiesBody: some View {
        if let callable {
            // One Form row per tool, and no explicit `Divider`.
            //
            // A Divider between entries is a SIBLING of them, so a grouped
            // Form lays it out as a row of its own -- with a row's minimum
            // height and a row's padding around a one-pixel line. That was
            // the empty band between every pair of tools. The grouped style
            // already separates its rows; the extra one only added the gap.
            ForEach(callable) { tool in
                toolRow(tool)
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
        VStack(alignment: .leading, spacing: 3) {
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

            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(tool.summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if tool.hasMoreDetail {
                    Image(systemName: "ellipsis.circle")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            .help(tool.about)

            if !tool.cli.available, let reason = tool.cli.reason {
                // Where a command can be called from. The registry's stated
                // reason is kept, but on hover: it is written for whoever
                // maintains the table, and in the row it read as an apology
                // under every second entry.
                Text("Socket and MCP only")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .help(reason)
            }
        }
        .padding(.vertical, 1)
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
