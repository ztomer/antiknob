// StatusParse.swift — reading a device status payload into what the UI shows.
//
// This was written out TWICE inside `ConfigStore.refreshStatus()`, once for
// the daemon's reply and once for the CLI's, differing only in that the
// daemon carries tap fields. Two copies of one parser is a defect waiting
// for someone to fix a field in one of them: the product-name fallback
// chain, the transport-from-power fallback and the "devices present means
// hardware found" rule all had to be kept identical by hand, and nothing
// checked that they were.
//
// One parser, called from both branches. The CLI payload simply has no tap
// keys, so it takes the defaults -- which is the correct reading, because a
// CLI that never had an event tap cannot report on one.

import Foundation

/// Everything a status payload says about the hardware, as a value.
public struct ParsedStatus: @unchecked Sendable, Equatable {
    public var hardwareFound: Bool = false
    public var product: String = "Not detected"
    public var transport: String = "disconnected"
    public var powerDescription: String = "Wired (USB Bus Powered)"
    public var devices: [[String: Any]] = []
    public var tapActive: Bool = false
    public var tapError: String?

    public static func == (lhs: ParsedStatus, rhs: ParsedStatus) -> Bool {
        lhs.hardwareFound == rhs.hardwareFound
            && lhs.product == rhs.product
            && lhs.transport == rhs.transport
            && lhs.powerDescription == rhs.powerDescription
            && lhs.devices.count == rhs.devices.count
            && lhs.tapActive == rhs.tapActive
            && lhs.tapError == rhs.tapError
    }
}

public enum StatusParse {
    /// The model name shown when a device is present but unnamed. Never
    /// used for an ABSENT device: "Not detected" and a model name are
    /// different claims and must not be reachable from the same state.
    static let unnamedDevice = "Anticater VK-01"

    /// Read a status payload. Anything missing keeps its default, so a
    /// partial reply degrades field by field instead of being discarded.
    public static func parse(_ status: [String: Any]) -> ParsedStatus {
        var out = ParsedStatus()

        if let active = status["tap_active"] as? Bool {
            out.tapActive = active
        }
        if let error = status["tap_error"] as? String {
            out.tapError = error
        }
        if let transport = status["transport"] as? String {
            out.transport = transport
        }
        if let power = status["power"] as? [String: Any] {
            if let description = power["description"] as? String {
                out.powerDescription = description
            }
            // Only as a fallback: the top-level transport is the daemon's
            // own answer, and the power block's copy is a restatement of
            // it. Letting the copy win would make a stale power reading
            // able to contradict a fresh transport one.
            if out.transport == "disconnected", let transport = power["transport"] as? String {
                out.transport = transport
            }
        }
        if let devices = status["devices"] as? [[String: Any]], !devices.isEmpty {
            out.hardwareFound = true
            out.devices = devices
            out.product = productName(of: devices[0])
        }
        return out
    }

    /// The name to show for a device that is definitely present.
    ///
    /// `name` first, `product_string` second, the model name last. The
    /// empty-string checks matter: a device reporting `"name": ""` would
    /// otherwise blank the banner rather than falling through.
    static func productName(of device: [String: Any]) -> String {
        if let name = device["name"] as? String, !name.isEmpty {
            return name
        }
        if let name = device["product_string"] as? String, !name.isEmpty {
            return name
        }
        return unnamedDevice
    }
}
