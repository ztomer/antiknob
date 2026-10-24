// A hand-written fake of the daemon, for ConfigStore's RPC paths.
//
// Records every call and answers from a script; a `failure` makes every call
// throw it. Locked, because the store calls it from detached tasks while the
// test reads it from the main actor.

import Foundation

@testable import AntiknobUI

final class FakeDaemonRPC: DaemonRPC, @unchecked Sendable {
    private let lock = NSLock()
    private var _calls: [String] = []

    /// Every call so far, as `name(args)`, in order.
    var calls: [String] { lock.withLock { _calls } }

    /// When set, every call that can throw throws this.
    var failure: Error? {
        get { lock.withLock { _failure } }
        set { lock.withLock { _failure = newValue } }
    }
    private var _failure: Error?

    // The script. Plain stored properties, set before the call under test.
    var connected = true
    var status: [String: Any] = [:]
    var config: Config?
    var appliedLedMode = 1
    var ledReply: [String: Any] = [:]
    var commands: [DaemonCommand] = []
    var knobModeReply: [String: Any] = [:]
    var bindMessage = "Flashed 15 slot bindings"
    var uploadMessage = "Keymap flashed"
    var slotsReply: [String: Any] = [:]
    var rawReply: [String: Any] = [:]
    var path: String?

    private func record(_ call: String) throws {
        let fail: Error? = lock.withLock {
            _calls.append(call)
            return _failure
        }
        if let fail { throw fail }
    }

    func isConnected() -> Bool {
        lock.withLock { _calls.append("isConnected()") }
        return connected && failure == nil
    }

    func getStatus() throws -> [String: Any] {
        try record("getStatus()")
        return status
    }

    func getConfig() throws -> Config {
        try record("getConfig()")
        guard let config else { throw SocketError.serverError("no config scripted") }
        return config
    }

    func setConfig(_ config: Config) throws { try record("setConfig()") }

    func setLayer(_ layerIndex: Int) throws { try record("setLayer(\(layerIndex))") }

    func setLed(layer: Int, mode: String) throws -> AppliedLedMode {
        try record("setLed(\(layer), \(mode))")
        return AppliedLedMode(mode: appliedLedMode)
    }

    func getLed(layer: Int) throws -> [String: Any] {
        try record("getLed(\(layer))")
        return ledReply
    }

    func listCommands() throws -> [DaemonCommand] {
        try record("listCommands()")
        return commands
    }

    func getKnobMode() throws -> [String: Any] {
        try record("getKnobMode()")
        return knobModeReply
    }

    func bindSlots(layer: Int?) throws -> String {
        try record("bindSlots(\(layer.map(String.init) ?? "all"))")
        return bindMessage
    }

    func uploadKeymap(yaml: String, layer: Int?) throws -> String {
        try record("uploadKeymap(\(yaml), \(layer.map(String.init) ?? "all"))")
        return uploadMessage
    }

    func readSlots(group: UInt8?, counters: [UInt8]?) throws -> [String: Any] {
        try record("readSlots(\(group.map(String.init) ?? "-"), \(counters.map { "\($0)" } ?? "-"))")
        return slotsReply
    }

    func sendRaw(bytes: [String]) throws -> [String: Any] {
        try record("sendRaw(\(bytes.joined(separator: " ")))")
        return rawReply
    }

    var connectedPath: String? { lock.withLock { path } }

    func loadConfigFromDisk() -> Config? { nil }

    func saveConfigToDisk(_ config: Config) throws { try record("saveConfigToDisk()") }
}
