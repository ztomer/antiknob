// ConfigStore+Launch.swift — the daemon as a PROCESS, not as a peer.
//
// Split from ConfigStore for the type-body-length cap, along a real seam:
// nothing here speaks the socket protocol or touches the config. It asks
// launchctl whether the login item exists, installs or removes it, and
// kickstarts the daemon -- three things that happen to be about the same
// program the rest of the store talks to, and share no state with it.

import Foundation

extension ConfigStore {
    func checkStartOnLogin() {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let plistPath = "\(home)/Library/LaunchAgents/com.antiknob.daemon.plist"
        self.startOnLogin = FileManager.default.fileExists(atPath: plistPath)
    }

    func toggleStartOnLogin(enabled: Bool) {
        Task.detached(priority: .userInitiated) {
            let daemonPath = Self.resolveDaemonPath()
            let task = Process()
            task.executableURL = URL(fileURLWithPath: daemonPath)
            task.arguments = [enabled ? "--install-login-item" : "--uninstall-login-item"]
            try? task.run()
            task.waitUntilExit()
            await MainActor.run { [weak self] in
                self?.checkStartOnLogin()
            }
        }
    }

    /// Where the daemon binary is on THIS machine, for the MCP config the
    /// Services pane hands out. It was hardcoded to
    /// `/Applications/Antiknob/bin/antiknob-daemon`, which is one of the
    /// four places `resolveDaemonPath` already looks -- so the config the
    /// pane copied could name a binary that is not there.
    var daemonBinaryPath: String { Self.resolveDaemonPath() }

    nonisolated static func resolveDaemonPath() -> String {
        let possiblePaths = [
            "/Applications/Antiknob/bin/antiknob-daemon",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.local/bin/antiknob-daemon",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.cargo/bin/antiknob-daemon",
            "/usr/local/bin/antiknob-daemon"
        ]
        if let found = possiblePaths.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) {
            return found
        }
        return "/Applications/Antiknob/bin/antiknob-daemon"
    }
}
