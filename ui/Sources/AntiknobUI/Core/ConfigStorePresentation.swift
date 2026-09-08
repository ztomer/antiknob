// ConfigStorePresentation.swift — the store's read-only view of itself.
//
// Split out of ConfigStore for the type-body-length gate, along the seam the
// backlog already named: none of this touches the socket, the config file or
// the poll timer. It maps state the store already holds into what the views
// render, and every piece of it is pinned by tests that need no store.

import SwiftUI

extension ConfigStore {
    /// Whether the host layers on screen can actually fire, as a value the
    /// views render without deciding anything themselves.
    var modePresentation: ModePresentation {
        ModePresentation(rawMode: knobModeRaw, deviceBinding: deviceBindingSummary)
    }

    var status: StatusPresentation {
        StatusPresentation(
            transport: transport,
            powerDescription: powerDescription,
            connected: hardwareConnected
        )
    }

    var transportDisplay: String { status.transportDisplay }
    var transportIcon: String { status.transportIcon }
    var powerIcon: String { status.powerIcon }

    /// Colour stays here: `Color` is SwiftUI, and `StatusPresentation` is
    /// deliberately free of it so the mapping tests need no view stack. The
    /// switch is over `Transport`, not over the raw string, so a new case
    /// fails to compile here rather than quietly rendering grey.
    var transportColor: Color {
        switch status.link {
        case .usb: return .green
        case .wireless24GHz: return .cyan
        case .bluetooth: return .blue
        case nil: return .secondary
        }
    }
}
