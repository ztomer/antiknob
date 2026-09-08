// Gesture.swift — the knob's five gesture slots.
//
// Split from Models.swift for the file-length gate.
//
// All five are real. An earlier version of this file carried an `isBindable`
// flag reporting hold+twist as unreachable, and the layer view dimmed those
// rows. That was wrong: the five gestures are key IDs 2..6 in the device's
// 0xFD command space, captured from the vendor app driving this hardware.
// The claim came from a probe of the WRONG command space returning nothing,
// which is not evidence of absence.

import Foundation

enum Gesture: String, CaseIterable, Identifiable, Hashable, Sendable {
    case twistL
    case twistR
    case holdTwistL
    case holdTwistR
    case press

    var id: String { rawValue }
}

let gestureTitles: [Gesture: String] = [
    .twistL: "Twist Left",
    .twistR: "Twist Right",
    .holdTwistL: "Hold + Twist Left",
    .holdTwistR: "Hold + Twist Right",
    .press: "Press"
]
