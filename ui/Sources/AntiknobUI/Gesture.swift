// Gesture.swift — the knob's five gesture slots, and which the firmware can
// actually produce.
//
// Split from Models.swift for the file-length gate, along a real seam: this
// is the one type in there that carries a hardware fact rather than a config
// shape.

import Foundation

enum Gesture: String, CaseIterable, Identifiable, Hashable, Sendable {
    case twistL
    case twistR
    case holdTwistL
    case holdTwistR
    case press

    var id: String { rawValue }

    /// Whether the knob's firmware can actually produce this gesture.
    ///
    /// It cannot produce hold+twist. The CH57x knob vocabulary is exactly
    /// CCW, press and CW -- confirmed in the reference protocol and on this
    /// hardware, where a hold-and-turn emits the press binding followed by
    /// the rotate binding, and a probe that wrote distinct markers to the
    /// two spare firmware slots saw neither fire.
    ///
    /// The cases stay so configs carrying bindings for them keep loading.
    /// Showing them as though they will fire is the defect.
    var isBindable: Bool {
        self != .holdTwistL && self != .holdTwistR
    }

    /// Why it cannot fire, for anything that shows it to someone.
    var unavailableReason: String? {
        isBindable ? nil
            : "The knob's firmware has no hold+twist gesture — it reports a "
              + "hold-and-turn as a press followed by a rotation."
    }
}

let gestureTitles: [Gesture: String] = [
    .twistL: "Twist Left",
    .twistR: "Twist Right",
    .holdTwistL: "Hold + Twist Left",
    .holdTwistR: "Hold + Twist Right",
    .press: "Press"
]
