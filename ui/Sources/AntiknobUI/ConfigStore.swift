// ConfigStore.swift — Central reactive state store for Antiknob.
// Bridges the SwiftUI UI with the daemon socket and local config file.

import AppKit
import Foundation
import SwiftUI

private struct StatusSnapshot: @unchecked Sendable {
    let connected: Bool
    let hwFound: Bool
    let product: String
    let transport: String
    let powerDesc: String
    let devices: [[String: Any]]
    let tapActive: Bool
    let tapError: String?
    let activeLayer: Int?
}

struct SlotDumpBox: @unchecked Sendable {
    let slots: [[String: Any]]
}

@MainActor
public final class ConfigStore: ObservableObject {
    public static let shared = ConfigStore()

    @Published var cfg: Config = Config.defaultPOC {
        didSet {
            if !syncing {
                applyConfig(cfg)
                lastSaved = Date()
            }
        }
    }

    @Published var daemonConnected: Bool = false
    @Published var hardwareConnected: Bool = false
    @Published var hardwareProduct: String = "Not detected"
    @Published var transport: String = "disconnected"
    @Published var powerDescription: String = "Wired (USB Bus Powered)"
    @Published var devices: [[String: Any]] = []
    @Published var tapActive: Bool = false
    @Published var tapError: String?
    @Published var lastSaved: Date?
    @Published var activeLayerIdx: Int = 0
    @Published var statusMessage: String?
    @Published var isBindingSlots: Bool = false
    /// What the knob's firmware will do with a gesture. Deliberately NOT on
    /// the 2-second status poll: reading it walks the device's slot table,
    /// and hammering the HID device to redraw a banner that changes only
    /// when the firmware is reflashed would be a poor trade. Refreshed on
    /// launch, on becoming active, and after any flash.
    @Published var knobModeRaw: String?
    /// Which device layer the daemon hears, in the daemon's own words.
    /// nil until the knob's bindings have been read, which presents as
    /// "not stated" rather than as an arrangement nobody confirmed.
    @Published var deviceBindingSummary: String?
    /// How many buttons the installed layout declares, and the key IDs that
    /// puts the knob's five gestures on. Read from the daemon rather than
    /// assumed: the count places the slots, and a layout declaring the wrong
    /// number writes well-formed packets into slots the firmware never
    /// reads, with nothing anywhere reporting a failure.
    @Published var knobButtons: Int?
    @Published var knobKeyIds: [Int]?
    /// Which DEVICE layer carries the slot chords, and therefore the one
    /// whose backlight the daemon drives. nil when nothing is bound, which
    /// is a real state: no layer's colour can appear until one is.
    @Published var boundDeviceLayer: Int?
    @Published var startOnLogin: Bool = false
    /// The socket path the last successful call actually went to. nil when
    /// nothing has answered. Three panes printed `/tmp/antiknob.sock` as a
    /// constant, on a machine where that symlink does not exist and every
    /// call was going to the Application Support socket instead.
    @Published var socketPath: String?

    /// Everything the status bar renders, as a pure value. Lives in
    /// `StatusPresentation` rather than here so it can be tested without
    /// standing up a store -- `init()` loads config and starts a poll timer.
    private var syncing = false
    private let client = SocketClient.shared
    private nonisolated(unsafe) var pollTimer: Timer?

    /// Whether this store is wired to the daemon and the config file.
    ///
    /// False only in tests. `init()` loads the real config and every `cfg`
    /// assignment writes it back, so a test that builds a store and sets
    /// `cfg` to a fixture EDITS THE USER'S CONFIGURATION -- which is not a
    /// hypothesis. A test of this file's own index safety renamed a layer to
    /// "Browse", set it green, and pushed both to the running daemon; the
    /// evidence is a `host.json.4` backup holding a layer nobody created.
    /// Three restores were destroyed before the backups made it visible.
    private let persists: Bool

    init() {
        persists = true
        loadInitial()
        startPolling()
    }

    /// A store that touches nothing: no load, no poll, no write.
    ///
    /// The only way to exercise this type without a daemon and a real
    /// `host.json` behind it.
    init(inMemory config: Config) {
        persists = false
        syncing = true
        cfg = config
        syncing = false
    }

    deinit {
        pollTimer?.invalidate()
    }

    // MARK: - Initial Load and Polling

    func loadInitial() {
        syncing = true
        defer { syncing = false }

        // Attempt daemon load first
        if let config = try? client.getConfig() {
            self.cfg = config
            self.daemonConnected = true
        } else if let diskConfig = client.loadConfigFromDisk() {
            self.cfg = diskConfig
            self.daemonConnected = false
        } else {
            self.cfg = Config.defaultPOC
            self.daemonConnected = false
        }

        checkStartOnLogin()
        refreshStatus()
        refreshKnobMode()
    }

    /// Read the firmware's slot table and record what it says. Failure
    /// leaves the mode nil, which presents as "unknown" -- never as one of
    /// the two real modes, because not knowing is not the same as knowing.
    func refreshKnobMode() {
        Task.detached(priority: .utility) {
            let reply = try? SocketClient.shared.getKnobMode()
            let mode = reply?["mode"] as? String
            let binding = reply?["device_binding_summary"] as? String
            let buttons = reply?["buttons"] as? Int
            let keyIds = reply?["knob_key_ids"] as? [Int]
            let bound = (reply?["device_binding"] as? [String: Any])?["host_translated"] as? Int
            await MainActor.run { [weak self] in
                self?.knobModeRaw = mode
                self?.deviceBindingSummary = binding
                self?.knobButtons = buttons
                self?.knobKeyIds = keyIds
                self?.boundDeviceLayer = bound
            }
        }
    }

    private func startPolling() {
        pollTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshStatus()
            }
        }
        NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshStatus()
                self?.refreshKnobMode()
                self?.adoptDaemonConfigIfChanged()
            }
        }
    }

    /// Take the daemon's config if it has changed while this window was in
    /// the background.
    ///
    /// This store held its copy for the app's whole life and wrote it back
    /// on every edit, so a config changed underneath it -- by the CLI, by a
    /// hand edit, by another copy of this app -- was silently overwritten
    /// the next time anything here was touched. That is not theoretical: a
    /// restored config was destroyed exactly this way on 2026-09-08, by a
    /// window sitting in the background holding a stale copy.
    ///
    /// Only on activation, and only when it differs. Every edit here saves
    /// immediately, so this store is never AHEAD of the daemon -- there is
    /// no unsaved work to protect, and doing it on the two-second poll would
    /// race a keystroke's own save and yank the text field out from under
    /// the person typing.
    func adoptDaemonConfigIfChanged() {
        guard let fromDaemon = try? client.getConfig(), fromDaemon != cfg else { return }
        syncing = true
        defer { syncing = false }
        cfg = fromDaemon
    }

    func refreshStatus() {
        Task.detached(priority: .utility) {
            let client = SocketClient.shared
            let connected = client.isConnected()
            // One parser for both sources; see `StatusParse`. The daemon is
            // asked first and the CLI is the fallback for when it is down.
            let payload: [String: Any]? = connected
                ? try? client.getStatus()
                : Self.queryCliStatus()
            let parsed = payload.map(StatusParse.parse) ?? ParsedStatus()

            let snapshot = StatusSnapshot(
                connected: connected,
                hwFound: parsed.hardwareFound,
                product: parsed.product,
                transport: parsed.transport,
                powerDesc: parsed.powerDescription,
                devices: parsed.devices,
                tapActive: parsed.tapActive,
                tapError: parsed.tapError,
                activeLayer: parsed.activeLayer
            )

            await MainActor.run { [weak self] in
                guard let self = self else { return }
                self.daemonConnected = snapshot.connected
                self.hardwareConnected = snapshot.hwFound
                self.hardwareProduct = snapshot.product
                self.transport = snapshot.transport
                self.powerDescription = snapshot.powerDesc
                self.devices = snapshot.devices
                self.tapActive = snapshot.tapActive
                self.tapError = snapshot.tapError
                self.socketPath = SocketClient.shared.connectedPath
                // The daemon's answer, not the app's memory of what was
                // clicked. The menu bar's checkmark used to track a local
                // variable nothing else read or wrote.
                if let active = snapshot.activeLayer {
                    self.activeLayerIdx = active
                }
                self.checkStartOnLogin()
            }
        }
    }

    private nonisolated static func queryCliStatus() -> [String: Any]? {
        let task = Process()
        let pipe = Pipe()

        let possiblePaths = CliDiscovery.searchPaths(
            home: FileManager.default.homeDirectoryForCurrentUser.path,
            resourcePath: Bundle.main.resourcePath,
            besideApp: Bundle.main.bundleURL
                .deletingLastPathComponent()
                .appendingPathComponent("antiknob").path
        )
        guard let execPath = CliDiscovery.resolve(
            paths: possiblePaths,
            isExecutable: { FileManager.default.isExecutableFile(atPath: $0) }
        ) else {
            return nil
        }

        task.executableURL = URL(fileURLWithPath: execPath)
        task.arguments = ["status", "--json"]
        task.standardOutput = pipe
        task.standardError = Pipe()

        do {
            try task.run()
            task.waitUntilExit()
            guard task.terminationStatus == 0 else { return nil }
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            return CliDiscovery.decodeStatus(data)
        } catch {
            return nil
        }
    }

    func openAccessibilitySettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
            NSWorkspace.shared.open(url)
        }
    }

    func restartDaemon() {
        Task.detached(priority: .userInitiated) {
            let task = Process()
            task.executableURL = URL(fileURLWithPath: "/bin/launchctl")
            let uid = getuid()
            task.arguments = ["kickstart", "-k", "gui/\(uid)/com.antiknob.daemon"]
            try? task.run()
            task.waitUntilExit()
            try? await Task.sleep(for: .milliseconds(500))
            await MainActor.run { [weak self] in
                self?.refreshStatus()
            }
        }
    }

    // MARK: - Config Persistence & Mutation

    /// How many times this store has set out to persist its config.
    ///
    /// Counted SYNCHRONOUSLY, because the write itself is a detached Task
    /// and a test that reads `host.json` straight afterwards sees the file
    /// as it was -- which is how the first version of the guard against this
    /// passed with the guard deleted. A counter the write increments before
    /// it goes async is a signal a test can actually see.
    private(set) var persistAttempts = 0

    func applyConfig(_ newConfig: Config) {
        // An in-memory store never reaches the daemon or the disk. Without
        // this, every test that assigns `cfg` rewrites the user's config.
        guard persists else { return }
        persistAttempts += 1
        let connected = self.daemonConnected
        Task.detached(priority: .userInitiated) {
            let client = SocketClient.shared
            do {
                if connected {
                    try client.setConfig(newConfig)
                } else {
                    try client.saveConfigToDisk(newConfig)
                }
            } catch {
                // Fallback to disk if socket failed
                try? client.saveConfigToDisk(newConfig)
            }
        }
    }

    public func reloadFromSource() {
        syncing = true
        defer { syncing = false }
        if let config = try? client.getConfig() {
            self.cfg = config
            self.daemonConnected = true
        } else if let diskConfig = client.loadConfigFromDisk() {
            self.cfg = diskConfig
        }
        refreshStatus()
    }

    // MARK: - Device Operations

}
