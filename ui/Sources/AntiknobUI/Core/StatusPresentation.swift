// StatusPresentation.swift — the link the knob is on, and the status bar's
// read of the device, as pure values.
//
// Split out of Models.swift, which sat exactly on the 500-line cap; nothing
// here is part of the host.json schema the Swift/Rust parity gate reads.

import Foundation

/// The link the knob is on.
///
/// Raw values are the daemon's `transport` strings. Parsing once, here, is
/// what lets every mapping below switch EXHAUSTIVELY: adding a transport
/// becomes a compile error at each site that has to render it, instead of a
/// silent `default:` fall-through to "Disconnected". The colour mapping lives
/// on `ConfigStore` (it needs SwiftUI) and is switched over this same enum,
/// so the two cannot drift apart the way two string switches did.
enum Transport: String, CaseIterable, Sendable {
    case usb
    case wireless24GHz = "wireless_2_4g"
    case bluetooth

    var displayName: String {
        switch self {
        case .usb: return "USB (Wired)"
        case .wireless24GHz: return "2.4GHz Wireless"
        case .bluetooth: return "Bluetooth Wireless"
        }
    }

    var icon: String {
        switch self {
        case .usb: return "cable.connector"
        case .wireless24GHz: return "antenna.radiowaves.left.and.right"
        case .bluetooth: return "wave.3.right"
        }
    }
}

/// The status bar's read of the device, as a pure value.
///
/// Transport and power render as SF Symbol glyphs with the words in the
/// tooltip, so glyph and tooltip must come from one place or they drift.
/// `powerIcon` is derived from the daemon's own `powerDescription` for
/// exactly that reason -- it cannot disagree with the text beside it.
struct StatusPresentation: Equatable, Sendable {
    let transport: String
    let powerDescription: String
    let connected: Bool

    /// nil when the daemon reports a link this build does not know (including
    /// its "disconnected" sentinel).
    var link: Transport? { Transport(rawValue: transport) }

    var transportDisplay: String { link?.displayName ?? "Disconnected" }
    var transportIcon: String { link?.icon ?? "circle.slash" }

    var powerIcon: String {
        guard connected else { return "powerplug.slash" }
        return powerDescription.localizedCaseInsensitiveContains("battery")
            ? "battery.100"
            : "powerplug.fill"
    }
}
