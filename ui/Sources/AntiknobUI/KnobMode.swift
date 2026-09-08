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

    /// One line of STATUS, not a warning.
    ///
    /// This was `banner`: an orange triangle and "These layers are inactive
    /// -- the knob is flashed standalone." A knob out of the box talks to
    /// macOS; flashing it once is a setup step, like granting Accessibility.
    /// Dressing that as a fault made a working app look broken on first run.
    ///
    /// Still three distinct states, and `unknown` still says it does not
    /// know rather than picking one of the other two -- "not read yet" and
    /// "not flashed" are different facts about the hardware.
    var statusText: String {
        switch mode {
        case .hostTranslate: return "Flashed — sending gestures to Antiknob"
        case .standalone: return "Not flashed — gestures still go to macOS"
        case .unknown: return "Not read yet"
        }
    }

    /// What the button beside that status offers to do.
    var flashActionTitle: String {
        hostLayersCanFire ? "Reflash" : "Flash Knob"
    }

}
