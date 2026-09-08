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

    /// Which DEVICE layer the daemon hears, in the daemon's own words.
    ///
    /// Separate from `mode` because the two answer different questions.
    /// `mode` says whether the knob is flashed to send slot chords at all;
    /// this says which of the three firmware layers carries them, and
    /// therefore which layers run standalone with the daemon stopped. The
    /// app presented all three host layers as though the daemon drove all
    /// three, which is true of none of them.
    let deviceBinding: String?

    init(rawMode: String?, deviceBinding: String? = nil) {
        self.mode = rawMode.flatMap(KnobMode.init(rawValue:)) ?? .unknown
        // An empty string is not a summary. Treated as absent so the view
        // never renders a blank row where an explanation should be.
        let trimmed = deviceBinding?.trimmingCharacters(in: .whitespacesAndNewlines)
        self.deviceBinding = (trimmed?.isEmpty ?? true) ? nil : trimmed
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

    /// True when there is anything to draw at all.
    private var hasSomethingToSay: Bool {
        mode.banner != nil || mode.deviceBinding != nil
    }

    var body: some View {
        if hasSomethingToSay {
            content
        }
    }

    @ViewBuilder
    private var content: some View {
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
                        if let binding = mode.deviceBinding {
                            Text(binding)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
        } else if let binding = mode.deviceBinding {
            // Nothing is wrong, but the arrangement is still worth stating:
            // two of the three layers run with the daemon stopped, and
            // nothing else in the app says so.
            Section {
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "info.circle")
                        .foregroundStyle(.secondary)
                    Text(binding)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}
