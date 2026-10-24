// DaemonRPC.swift — the daemon calls the store makes, as a narrow protocol.
//
// `ConfigStore`'s one-shot commands used to reach `SocketClient.shared`
// directly, so every success path and every failure path ran through a live
// `/tmp/antiknob.sock`: nothing could exercise them, and the mapping from an
// RPC outcome to the message the user reads (`statusMessage`, the completion
// result) was verified by nobody. The protocol is the seam: the socket
// client conforms, a test supplies a hand-written fake, and a store built
// for tests (`ConfigStore(inMemory:)`) defaults to `OfflineRPC`, which can
// reach nothing -- so a test that forgets to supply a fake fails loudly
// instead of talking to the daemon on the machine it runs on.

import Foundation

/// Every daemon call `ConfigStore` and the Services pane make.
///
/// Synchronous and throwing, like the client: callers hop off the main
/// actor themselves (`Task.detached`), which is where the blocking socket
/// read belongs.
protocol DaemonRPC: Sendable {
    func isConnected() -> Bool
    func getStatus() throws -> [String: Any]
    func getConfig() throws -> Config
    func setConfig(_ config: Config) throws
    func setLayer(_ layerIndex: Int) throws
    func setLed(layer: Int, mode: String) throws -> AppliedLedMode
    func getLed(layer: Int) throws -> [String: Any]
    func listCommands() throws -> [DaemonCommand]
    func getKnobMode() throws -> [String: Any]
    func bindSlots(layer: Int?) throws -> String
    func uploadKeymap(yaml: String, layer: Int?) throws -> String
    func readSlots(group: UInt8?, counters: [UInt8]?) throws -> [String: Any]
    func sendRaw(bytes: [String]) throws -> [String: Any]
    /// The socket the last successful call went to, for display.
    var connectedPath: String? { get }
    func loadConfigFromDisk() -> Config?
    func saveConfigToDisk(_ config: Config) throws
}

// The client's own signatures (defaults and all) already satisfy every
// requirement; this line is the whole conformance.
extension SocketClient: DaemonRPC {}

/// A daemon that is not there. The default for a store built for tests:
/// every call fails the way an unreachable socket fails.
struct OfflineRPC: DaemonRPC {
    private var unreachable: SocketError { .cannotConnect("offline (no daemon in this store)") }

    func isConnected() -> Bool { false }
    func getStatus() throws -> [String: Any] { throw unreachable }
    func getConfig() throws -> Config { throw unreachable }
    func setConfig(_ config: Config) throws { throw unreachable }
    func setLayer(_ layerIndex: Int) throws { throw unreachable }
    func setLed(layer: Int, mode: String) throws -> AppliedLedMode { throw unreachable }
    func getLed(layer: Int) throws -> [String: Any] { throw unreachable }
    func listCommands() throws -> [DaemonCommand] { throw unreachable }
    func getKnobMode() throws -> [String: Any] { throw unreachable }
    func bindSlots(layer: Int?) throws -> String { throw unreachable }
    func uploadKeymap(yaml: String, layer: Int?) throws -> String { throw unreachable }
    func readSlots(group: UInt8?, counters: [UInt8]?) throws -> [String: Any] { throw unreachable }
    func sendRaw(bytes: [String]) throws -> [String: Any] { throw unreachable }
    var connectedPath: String? { nil }
    func loadConfigFromDisk() -> Config? { nil }
    func saveConfigToDisk(_ config: Config) throws { throw unreachable }
}
