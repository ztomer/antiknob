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

    /// One line, or nothing. nil when there is nothing worth interrupting
    /// the user about.
    ///
    /// This was a paragraph, and beneath it a second line repeating the way
    /// out and a third restating the arrangement -- four lines of prose at
    /// the top of every layer tab, saying one thing three times. The claim
    /// is true and stays; the essay does not. What it used to spell out now
    /// lives in `detail`, which the row carries as a tooltip.
    var banner: String? {
        switch mode {
        case .hostTranslate:
            return nil
        case .standalone:
            return "These layers are inactive — the knob is flashed standalone."
        case .unknown:
            return "Not yet known whether these layers are active."
        }
    }

    /// The long form, shown on hover rather than on arrival.
    var detail: String? {
        switch mode {
        case .hostTranslate:
            return nil
        case .standalone:
            return "The knob's gestures go straight to macOS and never reach "
                + "Antiknob, so nothing bound here can run. Flashing the slot "
                + "bindings makes the knob send the chords the daemon listens for."
        case .unknown:
            return "The knob's firmware bindings have not been read yet, so "
                + "the app cannot say whether these layers can fire. Reconnect "
                + "the knob, or refresh from the status bar."
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

/// Says, in one line, when a layer's bindings cannot fire.
///
/// Host layers only run if the firmware sends the bound slot chords. A
/// standalone-flashed knob keeps them saved and unreachable, and the layer
/// view used to draw them exactly as it draws live ones -- an intention
/// presented as a fact, which is how a knob doing nothing looked like a
/// correctly configured knob. So the warning stays. What went is its length:
/// a four-line block, identical on every layer tab, that restated the same
/// fact three ways and carried a device-layer summary belonging to the
/// hardware pane rather than to any one layer.
///
/// Renders nothing when the layers can fire.
struct ReachabilityNotice: View {
    let mode: ModePresentation
    @ObservedObject var store: ConfigStore

    var body: some View {
        if let banner = mode.banner {
            Section {
                HStack(spacing: 8) {
                    Image(systemName: mode.icon)
                        .foregroundStyle(mode.mode == .standalone ? Color.orange : .secondary)
                    Text(banner)
                        .font(.callout)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 8)
                    if mode.callToAction != nil {
                        fixButton
                    }
                }
                .help(mode.detail ?? banner)
                .accessibilityElement(children: .combine)
                .accessibilityLabel(mode.detail ?? banner)
            }
        }
    }

    /// The way out, inline. It used to be a sentence naming a button on
    /// another pane; a warning that can be acted on where it appears is one
    /// fewer place for the fix to be described instead of offered.
    private var fixButton: some View {
        Button {
            store.bindSlots()
        } label: {
            if store.isBindingSlots {
                HStack(spacing: 5) {
                    ProgressView().controlSize(.small)
                    Text("Flashing…")
                }
            } else {
                Text("Flash Slot Bindings")
            }
        }
        .controlSize(.small)
        .disabled(store.isBindingSlots || !store.hardwareConnected)
        .help(store.hardwareConnected
              ? "Flash ⌃⌥F16..F20 to the knob so the daemon hears its gestures"
              : "No knob detected")
    }
}
