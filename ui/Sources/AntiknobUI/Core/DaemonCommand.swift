// DaemonCommand.swift — one command as the daemon describes itself.
//
// The Services pane rendered a hand-written array of eleven `ApiToolInfo`
// values. The daemon exposes seventeen commands. The differences were not
// cosmetic: the list advertised a `list_devices` tool that has never existed
// on any surface, and it named three parameters that no schema carries
// (`yaml_content`, `dry_run`, and a `color` on `set_led`). Anyone reading
// that pane to write an MCP call would have written a call the daemon
// refuses.
//
// The daemon already declares every command once, in `src/api/registry.rs`,
// and generates both its CLI and its MCP tool list from that table. This is
// the same table over the socket, so the pane can render what the connected
// build actually has rather than what this build was written believing.

import Foundation

/// One parameter, as the daemon's registry declares it for this surface.
struct DaemonParam: Identifiable, Hashable, Sendable {
    let name: String
    /// "flag", "string", "integer", "integer[]" or "string[]".
    let kind: String
    let about: String
    let required: Bool

    var id: String { name }

    /// `name: kind` for the pill beside the command, with required ones
    /// unmarked and optional ones suffixed -- the same convention a schema
    /// uses, rather than the pane's old habit of appending "?" to whichever
    /// ones the author remembered.
    var signature: String { required ? "\(name): \(kind)" : "\(name): \(kind)?" }
}

/// Whether a command reaches a surface, and the daemon's stated reason when
/// it does not.
///
/// The reason is carried, not summarised. A command absent from a surface
/// on purpose and one nobody has wired yet look identical without it, which
/// is the whole point of the registry recording it.
struct DaemonReach: Hashable, Sendable {
    let available: Bool
    let reason: String?
}

/// One command the connected daemon exposes.
struct DaemonCommand: Identifiable, Hashable, Sendable {
    /// Canonical snake_case name: the MCP tool name and the socket method.
    let name: String
    let about: String
    /// The CLI spelling, which differs where the typed name is older than
    /// the table (`status`, not `get-status`).
    let cliName: String
    let cli: DaemonReach
    let mcp: DaemonReach
    let params: [DaemonParam]

    var id: String { name }

    init?(json: [String: Any]) {
        guard let name = json["name"] as? String else { return nil }
        self.name = name
        self.about = json["about"] as? String ?? ""
        self.cliName = json["cli_name"] as? String ?? name
        self.cli = DaemonCommand.reach(json["cli"])
        self.mcp = DaemonCommand.reach(json["mcp"])
        self.params = (json["params"] as? [[String: Any]] ?? []).compactMap { p in
            guard let name = p["name"] as? String else { return nil }
            return DaemonParam(
                name: name,
                kind: p["kind"] as? String ?? "string",
                about: p["about"] as? String ?? "",
                required: p["required"] as? Bool ?? false
            )
        }
    }

    private static func reach(_ value: Any?) -> DaemonReach {
        guard let dict = value as? [String: Any] else {
            return DaemonReach(available: false, reason: nil)
        }
        return DaemonReach(
            available: dict["available"] as? Bool ?? false,
            reason: dict["reason"] as? String
        )
    }

    /// The commands an agent or a script can call: everything the socket and
    /// the MCP server carry. The pane lists these; the CLI-only ones are
    /// reachable from a terminal and are not what "exposed capabilities"
    /// meant.
    static func callable(_ all: [DaemonCommand]) -> [DaemonCommand] {
        all.filter(\.mcp.available)
    }
}
