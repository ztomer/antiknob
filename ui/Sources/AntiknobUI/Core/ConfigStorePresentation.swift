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

// MARK: - Reaching a layer by index, safely

extension ConfigStore {
    /// The layer at `idx`, or `nil` when that index no longer exists.
    ///
    /// Views hold an index, and SwiftUI re-evaluates a body with the index
    /// it captured. Delete a layer and the removed view's body can run once
    /// more against the shortened array before the parent drops it -- and
    /// `cfg.layers[idx]` TRAPS there rather than returning nothing. That is
    /// not a hypothetical: it crashed the app on the delete confirmation,
    /// from `LayerLighting.selection`, with 39 more subscripts behind it
    /// waiting for the same moment.
    ///
    /// Writing through a stale index is a no-op for the same reason: the
    /// layer that write was meant for is gone.
    subscript(layer idx: Int) -> LayerConfig? {
        get { cfg.layers.indices.contains(idx) ? cfg.layers[idx] : nil }
        set {
            guard let newValue, cfg.layers.indices.contains(idx) else { return }
            cfg.layers[idx] = newValue
        }
    }

    /// A binding to one layer's name that survives the layer being deleted
    /// while the field is on screen.
    func layerName(_ idx: Int) -> Binding<String> {
        Binding(
            get: { self[layer: idx]?.name ?? "" },
            set: { self[layer: idx]?.name = $0 }
        )
    }
}
