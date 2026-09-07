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

final class SocketClient: @unchecked Sendable {
    static let shared = SocketClient()

    private let primaryPath = "/tmp/antiknob.sock"
    private var fallbackPath: String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return "\(home)/Library/Application Support/antiknob/antiknob.sock"
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

        let payload: [String: Any] = [
            "jsonrpc": "2.0",
            "id": reqId,
            "method": method,
            "params": params
        ]

        guard let jsonData = try? JSONSerialization.data(withJSONObject: payload),
              var jsonString = String(data: jsonData, encoding: .utf8) else {
            throw SocketError.parseError("Failed to serialize JSON-RPC request.")
        }
        jsonString.append("\n")

        // Try primary path then fallback path
        var lastErr: SocketError = .cannotConnect(primaryPath)
        for path in [primaryPath, fallbackPath] {
            do {
                return try sendAndReceive(path: path, request: jsonString, timeoutSecs: timeoutSecs)
            } catch let err as SocketError {
                lastErr = err
                continue
            } catch {
                lastErr = .cannotConnect(path)
                continue
            }
        }
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

        guard let json = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
            let respStr = String(data: responseData, encoding: .utf8) ?? "<binary>"
            throw SocketError.parseError("Invalid JSON from server: \(respStr)")
        }

        if let errObj = json["error"] as? [String: Any] {
            let msg = errObj["message"] as? String ?? "Unknown error"
            throw SocketError.serverError(msg)
        }

        return json["result"] ?? [:]
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

    func setLed(layer: Int, mode: String, color: String?) throws -> String {
        var params: [String: Any] = ["layer": layer, "mode": mode]
        if let color = color, !color.isEmpty {
            params["color"] = color
        }
        let res = try rpcCall(method: "set_led", params: params)
        return (res as? [String: Any])?["status"] as? String ?? "OK"
    }

    func getLed(layer: Int) throws -> [String: Any] {
        let res = try rpcCall(method: "get_led", params: ["layer": layer])
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
