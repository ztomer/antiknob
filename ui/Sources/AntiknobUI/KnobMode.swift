// KnobMode.swift — whether the knob's firmware can reach the host layers.
//
// Split out of Models.swift for the file-length gate, along a real seam:
// nothing here is a config model. It is the one question the layer view has
// to answer honestly before it draws a binding as live.

import SwiftUI

/// What the knob's firmware will do with a gesture, as the daemon reports it.
///
/// Host layers only run when the firmware sends the bound slot chords. A
/// standalone-flashed knob has the layers saved and unreachable, which the
/// app used to draw as though they were live -- the same defect as reporting
/// a tap that was never installed.
enum KnobMode: String, Sendable {
    case hostTranslate = "host-translate"
    case standalone
    case unknown
}

/// Everything the layer view needs to say about reachability, as a value.
///
/// Pure so every branch is testable without a device or a view stack; the
/// `unknown` case exists so the app can say "not sure yet" instead of
/// picking one of the other two.
struct ModePresentation: Equatable, Sendable {
    let mode: KnobMode

    init(rawMode: String?) {
        self.mode = rawMode.flatMap(KnobMode.init(rawValue:)) ?? .unknown
    }

    /// True only when the firmware can actually reach the host layers.
    var hostLayersCanFire: Bool { mode == .hostTranslate }

    /// nil when there is nothing worth interrupting the user about.
    var banner: String? {
        switch mode {
        case .hostTranslate:
            return nil
        case .standalone:
            return "These layers are inactive. The knob is flashed standalone, "
                + "so its gestures go straight to macOS and never reach Antiknob."
        case .unknown:
            return "Can't tell whether these layers are active — the knob's "
                + "bindings haven't been read yet."
        }
    }

    /// The way out, offered only when there is one.
    var callToAction: String? {
        mode == .standalone ? "Flash Slot Bindings" : nil
    }

    var icon: String {
        switch mode {
        case .hostTranslate: return "checkmark.circle"
        case .standalone: return "exclamationmark.triangle"
        case .unknown: return "questionmark.circle"
        }
    }
}

/// Says when a layer's bindings cannot fire, and why.
///
/// Host layers only run if the firmware sends the bound slot chords. A
/// standalone-flashed knob keeps them saved and unreachable, and the layer
/// view used to draw them exactly as it draws live ones -- an intention
/// presented as a fact, which is how a knob doing nothing looked like a
/// correctly configured knob. Renders nothing when the layers can fire.
struct ReachabilityNotice: View {
    let mode: ModePresentation

    var body: some View {
        if let banner = mode.banner {
            Section {
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: mode.icon)
                        .foregroundStyle(mode.mode == .standalone ? Color.orange : .secondary)
                    VStack(alignment: .leading, spacing: 6) {
                        Text(banner)
                            .font(.callout)
                            .fixedSize(horizontal: false, vertical: true)
                        if let action = mode.callToAction {
                            Text("Hardware → \(action) makes the knob send them.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }
}
