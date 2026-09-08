// SocketClient.swift — Unix domain socket JSON-RPC client for antiknob-daemon.
// Connects to /tmp/antiknob.sock and communicates with the single source of truth.
// Provides graceful fallback to local host.json when daemon is offline.

import Foundation

enum SocketError: LocalizedError {
    case cannotCreateSocket
    case cannotConnect(String)
    case sendFailed
    case receiveFailed
    case parseError(String)
    case serverError(String)

    var errorDescription: String? {
        switch self {
        case .cannotCreateSocket: return "Failed to create POSIX Unix socket."
        case .cannotConnect(let path): return "Could not connect to daemon socket at \(path)."
        case .sendFailed: return "Failed to send data through Unix socket."
        case .receiveFailed: return "Failed to receive reply from Unix socket."
        case .parseError(let msg): return "Socket reply parse error: \(msg)"
        case .serverError(let msg): return "Daemon error: \(msg)"
        }
    }
}

/// The JSON-RPC wire format, with no socket attached.
///
/// `SocketClient` is 250 lines that a unit test could not reach, because every
/// path ran through a live `/tmp/antiknob.sock`. The framing and the parsing
/// are pure functions of their inputs, though, and they are where the format
/// bugs live -- a mis-shaped request or an error reply read as a result is
/// silent, and only shows up as the UI quietly doing nothing. Splitting them
/// out is the whole seam: the socket keeps the I/O, this keeps the contract.
enum SocketWire {
    /// One JSON-RPC request, newline-terminated as the daemon's line reader
    /// expects.
    static func encodeRequest(
        id: Int,
        method: String,
        params: [String: Any]
    ) throws -> String {
        let payload: [String: Any] = [
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        ]
        // `isValidJSONObject` FIRST, and not for tidiness: `data(withJSONObject:)`
        // raises an ObjC NSException on an unencodable value rather than
        // throwing a Swift error, so `try?` does not catch it and the app dies
        // instead of reporting a failed call. Found by the test below, which
        // crashed the whole test process before this guard existed.
        guard JSONSerialization.isValidJSONObject(payload) else {
            throw SocketError.parseError("Request parameters are not JSON-encodable.")
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              var text = String(data: data, encoding: .utf8) else {
            throw SocketError.parseError("Failed to serialize JSON-RPC request.")
        }
        text.append("\n")
        return text
    }

    /// The `result` of a reply, or a thrown `SocketError`.
    ///
    /// An `error` object is thrown rather than returned: a caller that reads
    /// the reply as a result would treat a refusal as success.
    static func decodeResponse(_ data: Data) throws -> Any {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            let text = String(data: data, encoding: .utf8) ?? "<binary>"
            throw SocketError.parseError("Invalid JSON from server: \(text)")
        }
        if let errObj = json["error"] as? [String: Any] {
            let msg = errObj["message"] as? String ?? "Unknown error"
            throw SocketError.serverError(msg)
        }
        return json["result"] ?? [:]
    }
}

final class SocketClient: @unchecked Sendable {
    static let shared = SocketClient()

    /// Where the daemon's socket can be, in the order this client tries.
    ///
    /// `/tmp/antiknob.sock` is a convenience SYMLINK the daemon creates when
    /// it can; the real socket lives in Application Support. Three panes
    /// used to print the /tmp path as fact -- "Socket Path: /tmp/antiknob.sock",
    /// "Connected (/tmp/antiknob.sock)" -- on a machine where that symlink
    /// does not exist and every call was going to the fallback. A path the
    /// app prints should be the path the app used.
    private var candidatePaths: [String] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return [
            "/tmp/antiknob.sock",
            "\(home)/Library/Application Support/antiknob/antiknob.sock"
        ]
    }

    /// The path the last successful call went to, or nil if none has
    /// succeeded. Written on the socket queue and read on the main actor,
    /// which is why it is guarded rather than a plain `var`.
    private var lastGoodPath: String?

    /// The socket this client is actually talking to, for display. nil when
    /// no call has succeeded yet -- which reads as "not connected" rather
    /// than as a path nothing has been reached at.
    var connectedPath: String? {
        queue.sync { lastGoodPath }
    }

    var localConfigURL: URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return home.appendingPathComponent("Library/Application Support/antiknob/host.json")
    }

    private var nextReqId = 1
    private let queue = DispatchQueue(label: "com.antiknob.socket", qos: .userInitiated)

    // MARK: - Low-Level Unix Socket RPC

    func rpcCall(method: String, params: [String: Any] = [:], timeoutSecs: Int = 2) throws -> Any {
        let reqId: Int = queue.sync {
            let id = nextReqId
            nextReqId += 1
            return id
        }

        let jsonString = try SocketWire.encodeRequest(id: reqId, method: method, params: params)

        // The /tmp symlink first, the real socket second. Whichever answers
        // is recorded, so the Services pane can name the socket in use
        // instead of the one it hopes is there.
        let paths = candidatePaths
        var lastErr: SocketError = .cannotConnect(paths[0])
        for path in paths {
            do {
                let result = try sendAndReceive(
                    path: path, request: jsonString, timeoutSecs: timeoutSecs
                )
                queue.sync { lastGoodPath = path }
                return result
            } catch let err as SocketError {
                lastErr = err
                continue
            } catch {
                lastErr = .cannotConnect(path)
                continue
            }
        }
        queue.sync { lastGoodPath = nil }
        throw lastErr
    }

    private func sendAndReceive(path: String, request: String, timeoutSecs: Int) throws -> Any {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw SocketError.cannotCreateSocket }
        defer { close(fd) }

        var tv = timeval(tv_sec: timeoutSecs, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = path.utf8CString
        guard pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            throw SocketError.cannotConnect("Socket path too long: \(path)")
        }

        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                _ = pathBytes.withUnsafeBufferPointer { src in
                    memcpy(dest, src.baseAddress!, src.count)
                }
            }
        }

        let addrLen = socklen_t(MemoryLayout<sa_family_t>.size + pathBytes.count)
        let connectRes = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { saPtr in
                connect(fd, saPtr, addrLen)
            }
        }

        guard connectRes == 0 else {
            throw SocketError.cannotConnect(path)
        }

        // Send request line
        let reqData = [UInt8](request.utf8)
        let bytesSent = reqData.withUnsafeBufferPointer { buf in
            write(fd, buf.baseAddress!, buf.count)
        }
        guard bytesSent == reqData.count else {
            throw SocketError.sendFailed
        }

        // Read response until newline
        var responseData = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while true {
            let bytesRead = read(fd, &buffer, buffer.count)
            guard bytesRead > 0 else {
                if responseData.isEmpty { throw SocketError.receiveFailed }
                break
            }
            responseData.append(buffer, count: bytesRead)
            if responseData.contains(0x0A) { // '\n'
                break
            }
        }

        return try SocketWire.decodeResponse(responseData)
    }

    // MARK: - High Level Typed Operations

    func isConnected() -> Bool {
        do {
            _ = try rpcCall(method: "get_status", params: [:], timeoutSecs: 1)
            return true
        } catch {
            return false
        }
    }

    func getStatus() throws -> [String: Any] {
        guard let res = try rpcCall(method: "get_status") as? [String: Any] else {
            throw SocketError.parseError("Status result is not an object")
        }
        return res
    }

    func getConfig() throws -> Config {
        let res = try rpcCall(method: "get_config")
        let data = try JSONSerialization.data(withJSONObject: res)
        let decoder = JSONDecoder()
        return try decoder.decode(Config.self, from: data)
    }

    func setConfig(_ config: Config) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(config)
        guard let jsonObject = try? JSONSerialization.jsonObject(with: data) else {
            throw SocketError.parseError("Could not convert config to JSON object")
        }
        _ = try rpcCall(method: "set_config", params: ["config": jsonObject])
    }

    func setLayer(_ layerIndex: Int) throws {
        _ = try rpcCall(method: "set_layer", params: ["layer": layerIndex])
    }

    /// Set a layer's backlight mode. The reply is the daemon's read-back of
    /// what the firmware holds afterwards, so a write the device swallowed
    /// is reported as the mode it kept rather than as success.
    ///
    /// No colour parameter: this knob's modes each carry their own colour
    /// and the bytes are ignored, so an argument here could only ever name a
    /// colour that was never applied.
    func setLed(layer: Int, mode: String) throws -> AppliedLedMode {
        let res = try rpcCall(method: "set_led", params: ["layer": layer, "mode": mode])
        guard let dict = res as? [String: Any], let applied = dict["mode"] as? Int else {
            throw SocketError.parseError("set_led did not report the mode the device holds.")
        }
        return AppliedLedMode(mode: applied)
    }

    func getLed(layer: Int) throws -> [String: Any] {
        let res = try rpcCall(method: "get_led", params: ["layer": layer])
        return (res as? [String: Any]) ?? [:]
    }

    /// What this daemon can actually do, read from the command table that
    /// generates its own CLI and MCP surfaces.
    ///
    /// The Services pane used to carry a hand-written list of eleven tools.
    /// The daemon has seventeen. The list named a `list_devices` that has
    /// never existed, gave `upload_keymap` a `yaml_content` parameter it
    /// does not take, `set_layer` a `layer` where MCP wants `index`, and
    /// `set_led` a `color` the tool schema does not carry. A second
    /// hand-kept copy of a generated table is a list of things that are not
    /// true yet.
    func listCommands() throws -> [DaemonCommand] {
        let res = try rpcCall(method: "list_commands", timeoutSecs: 3)
        guard let dict = res as? [String: Any],
              let raw = dict["commands"] as? [[String: Any]] else {
            throw SocketError.parseError("list_commands did not return a command list.")
        }
        return raw.compactMap(DaemonCommand.init(json:))
    }

    /// Ask the daemon what the knob's firmware will actually do with a
    /// gesture. The layer view needs this before it can honestly draw a
    /// host layer as live.
    func getKnobMode() throws -> [String: Any] {
        let res = try rpcCall(method: "get_knob_mode", timeoutSecs: 5)
        return (res as? [String: Any]) ?? [:]
    }

    func bindSlots(layer: Int? = nil, dryRun: Bool = false) throws -> String {
        var params: [String: Any] = ["dry_run": dryRun]
        if let l = layer { params["layer"] = l }
        let res = try rpcCall(method: "bind_slots", params: params)
        return (res as? [String: Any])?["status"] as? String ?? "OK"
    }

    func uploadKeymap(yaml: String, layer: Int? = nil) throws -> String {
        var params: [String: Any] = ["yaml": yaml]
        if let l = layer { params["layer"] = l }
        let res = try rpcCall(method: "upload_keymap", params: params)
        return (res as? [String: Any])?["message"] as? String ?? "Keymap flashed"
    }

    func readSlots(group: UInt8? = nil, counters: [UInt8]? = nil) throws -> [String: Any] {
        var params: [String: Any] = [:]
        if let g = group { params["group"] = g }
        if let c = counters { params["counters"] = c }
        let res = try rpcCall(method: "read_slots", params: params)
        return (res as? [String: Any]) ?? [:]
    }

    func sendRaw(bytes: [String]) throws -> [String: Any] {
        let res = try rpcCall(method: "send_raw", params: ["bytes": bytes])
        return (res as? [String: Any]) ?? [:]
    }

    // MARK: - Direct Disk Fallback (when daemon is not running)

    func loadConfigFromDisk() -> Config? {
        guard let data = try? Data(contentsOf: localConfigURL) else { return nil }
        return try? JSONDecoder().decode(Config.self, from: data)
    }

    func saveConfigToDisk(_ config: Config) throws {
        let folder = localConfigURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(config)
        try data.write(to: localConfigURL, options: .atomic)
    }
}
