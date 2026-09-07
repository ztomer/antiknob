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
}

private struct SlotDumpBox: @unchecked Sendable {
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
    @Published var startOnLogin: Bool = false

    /// Everything the status bar renders, as a pure value. Lives in
    /// `StatusPresentation` rather than here so it can be tested without
    /// standing up a store -- `init()` loads config and starts a poll timer.
    private var syncing = false
    private let client = SocketClient.shared
    private nonisolated(unsafe) var pollTimer: Timer?

    init() {
        loadInitial()
        startPolling()
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
            let mode = (try? SocketClient.shared.getKnobMode())?["mode"] as? String
            await MainActor.run { [weak self] in
                self?.knobModeRaw = mode
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
            }
        }
    }

    func refreshStatus() {
        Task.detached(priority: .utility) {
            let client = SocketClient.shared
            let connected = client.isConnected()
            var hwFound = false
            var product = "Not detected"
            var detectedTransport = "disconnected"
            var detectedDevices: [[String: Any]] = []
            var tapIsActive = false
            var tapErrStr: String?
            var powerDesc = "Wired (USB Bus Powered)"

            if connected {
                if let status = try? client.getStatus() {
                    if let ta = status["tap_active"] as? Bool {
                        tapIsActive = ta
                    }
                    if let te = status["tap_error"] as? String {
                        tapErrStr = te
                    }
                    if let tr = status["transport"] as? String {
                        detectedTransport = tr
                    }
                    if let pow = status["power"] as? [String: Any] {
                        if let desc = pow["description"] as? String {
                            powerDesc = desc
                        }
                        if detectedTransport == "disconnected", let tr = pow["transport"] as? String {
                            detectedTransport = tr
                        }
                    }
                    if let devs = status["devices"] as? [[String: Any]], !devs.isEmpty {
                        hwFound = true
                        detectedDevices = devs
                        if let p = devs[0]["name"] as? String, !p.isEmpty {
                            product = p
                        } else if let p = devs[0]["product_string"] as? String, !p.isEmpty {
                            product = p
                        } else {
                            product = "Anticater VK-01"
                        }
                    }
                }
            } else if let cliStatus = Self.queryCliStatus() {
                if let tr = cliStatus["transport"] as? String {
                    detectedTransport = tr
                }
                if let pow = cliStatus["power"] as? [String: Any] {
                    if let desc = pow["description"] as? String {
                        powerDesc = desc
                    }
                    if detectedTransport == "disconnected", let tr = pow["transport"] as? String {
                        detectedTransport = tr
                    }
                }
                if let devs = cliStatus["devices"] as? [[String: Any]], !devs.isEmpty {
                    hwFound = true
                    detectedDevices = devs
                    if let p = devs[0]["name"] as? String, !p.isEmpty {
                        product = p
                    } else if let p = devs[0]["product_string"] as? String, !p.isEmpty {
                        product = p
                    } else {
                        product = "Anticater VK-01"
                    }
                }
            }

            let snapshot = StatusSnapshot(
                connected: connected,
                hwFound: hwFound,
                product: product,
                transport: detectedTransport,
                powerDesc: powerDesc,
                devices: detectedDevices,
                tapActive: tapIsActive,
                tapError: tapErrStr
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
                self.checkStartOnLogin()
            }
        }
    }

    private nonisolated static func queryCliStatus() -> [String: Any]? {
        let task = Process()
        let pipe = Pipe()

        var possiblePaths: [String] = [
            "/Applications/Antiknob/bin/antiknob",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.local/bin/antiknob",
            "\(FileManager.default.homeDirectoryForCurrentUser.path)/.cargo/bin/antiknob",
            "/usr/local/bin/antiknob"
        ]
        if let res = Bundle.main.resourcePath {
            possiblePaths.append("\(res)/antiknob")
        }
        let besideApp = Bundle.main.bundleURL.deletingLastPathComponent().appendingPathComponent("antiknob").path
        possiblePaths.append(besideApp)

        guard let execPath = possiblePaths.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else {
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
            if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                return obj
            } else if let devices = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] {
                return [
                    "connected": !devices.isEmpty,
                    "devices": devices,
                    "product_string": devices.first?["name"] as? String ?? "Anticater VK01"
                ]
            }
        } catch {
            return nil
        }
        return nil
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

    private nonisolated static func resolveDaemonPath() -> String {
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

    // MARK: - Config Persistence & Mutation

    func applyConfig(_ newConfig: Config) {
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

    func setLed(layer: Int, mode: String, color: String?) {
        Task.detached(priority: .userInitiated) {
            do {
                let status = try SocketClient.shared.setLed(layer: layer, mode: mode, color: color)
                await MainActor.run { [weak self] in
                    self?.statusMessage = status
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.statusMessage = "LED error: \(error.localizedDescription)"
                }
            }
        }
    }

    public func bindSlots(layer: Int? = nil) {
        isBindingSlots = true
        Task.detached(priority: .userInitiated) {
            do {
                let status = try SocketClient.shared.bindSlots(layer: layer, dryRun: false)
                await MainActor.run { [weak self] in
                    self?.isBindingSlots = false
                    self?.statusMessage = status
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.isBindingSlots = false
                    self?.statusMessage = "Bind error: \(error.localizedDescription)"
                }
            }
        }
    }

    func uploadKeymap(
        yaml: String,
        layer: Int? = nil,
        completion: @escaping @MainActor @Sendable (Result<String, Error>) -> Void
    ) {
        Task.detached(priority: .userInitiated) {
            do {
                let msg = try SocketClient.shared.uploadKeymap(yaml: yaml, layer: layer)
                await MainActor.run { [weak self] in
                    self?.statusMessage = msg
                    completion(.success(msg))
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.statusMessage = "Upload error: \(error.localizedDescription)"
                    completion(.failure(error))
                }
            }
        }
    }

    func readSlots(
        group: UInt8? = nil,
        counters: [UInt8]? = nil,
        completion: @escaping @MainActor @Sendable (Result<[[String: Any]], Error>) -> Void
    ) {
        Task.detached(priority: .userInitiated) {
            do {
                let res = try SocketClient.shared.readSlots(group: group, counters: counters)
                let slots = res["slots"] as? [[String: Any]] ?? []
                let box = SlotDumpBox(slots: slots)
                await MainActor.run {
                    completion(.success(box.slots))
                }
            } catch {
                await MainActor.run {
                    completion(.failure(error))
                }
            }
        }
    }

    func sendRawPacket(hexString: String, completion: @escaping @MainActor @Sendable (Result<String, Error>) -> Void) {
        let parts = hexString.components(separatedBy: CharacterSet.whitespacesAndNewlines.union(.punctuationCharacters))
            .filter { !$0.isEmpty }
        Task.detached(priority: .userInitiated) {
            do {
                let res = try SocketClient.shared.sendRaw(bytes: parts)
                let count = res["bytes_sent"] as? Int ?? parts.count
                await MainActor.run {
                    completion(.success("Sent \(count) bytes"))
                }
            } catch {
                await MainActor.run {
                    completion(.failure(error))
                }
            }
        }
    }

    func getHardwareLedMode(layer: Int, completion: @escaping @MainActor @Sendable (Int?) -> Void) {
        Task.detached(priority: .userInitiated) {
            let res = try? SocketClient.shared.getLed(layer: layer)
            let mode = res?["mode"] as? Int
            await MainActor.run {
                completion(mode)
            }
        }
    }
}
