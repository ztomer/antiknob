// LedModes.swift — the knob's backlight modes, as data.
//
// One table, holding every question the app used to answer in three places:
// which modes exist, what each is called, what number the firmware reports
// for it, and what it looks like. `ledModeNames` was a separate `[Int: String]`
// in Models.swift whose own comment claimed to mirror the Rust table and had
// drifted from it for months; the mode list in LedSection was a third copy
// that carried different descriptions again.
//
// No SwiftUI here on purpose: Core/ holds the half with no view declarations,
// so all of this is reachable from unit tests without a view stack. The view
// maps `RGB` to `Color` at the point of drawing and nowhere else.

import Foundation

/// A colour the way the firmware's palette carries it.
///
/// Components are 0...1 rather than bytes because every consumer is a
/// renderer; the wire format's bytes live on the Rust side, which is the
/// only place that builds packets.
struct RGB: Equatable, Hashable, Sendable {
    let red: Double
    let green: Double
    let blue: Double

    init(_ red: Double, _ green: Double, _ blue: Double) {
        self.red = red
        self.green = green
        self.blue = blue
    }

    /// The vendor's own rainbow, in its own order, captured off the wire
    /// from ANTICATER.app and mirrored from `src/led.rs`.
    static let vendorRainbow: [RGB] = [
        RGB(1.0, 0.0, 0.0),
        RGB(1.0, 0.50, 0.19),
        RGB(1.0, 1.0, 0.19),
        RGB(0.0, 1.0, 0.0),
        RGB(0.0, 1.0, 1.0),
        RGB(0.0, 0.0, 1.0)
    ]
}

/// What a mode looks like on this knob's light.
///
/// Descriptive, not aspirational: each case is what the owner watched the
/// knob do, and `sweep` says outright that its shape was never captured.
/// A preview that renders a guess as confidently as a measurement is the
/// same defect as a layer view drawing an unreachable binding as live.
enum LedAppearance: Hashable, Sendable {
    /// Unlit.
    case unlit
    /// One fixed colour, which is what modes 1 and 2 carry whatever colour
    /// bytes are sent to them.
    case steady(RGB)
    /// A brightness pulse in one colour.
    case pulse(RGB)
    /// Steps through a captured palette in order.
    case palette([RGB])
    /// A continuous hue sweep, drawn as a stand-in for a pattern nobody
    /// has captured off the wire.
    case sweep

    /// True when the preview is a stand-in rather than a reproduction.
    /// The pane labels these so nobody reads the animation as evidence.
    var isApproximate: Bool { self == .sweep }
}

/// One selectable backlight mode.
struct LedMode: Identifiable, Hashable, Sendable {
    /// The wire name `set_led` takes, and the id used everywhere in the UI.
    let id: String
    let name: String
    let desc: String
    let icon: String
    /// The number `get_led` reports for this mode.
    let number: Int
    let appearance: LedAppearance

    /// The modes this device has, in mode-number order.
    ///
    /// Names and numbers mirror `LED_MODE_NAMES` in `src/led.rs`, which is
    /// the table that builds the packets; `LedModeTests` pins the pairing so
    /// the two cannot drift the way this list and `ledModeNames` did.
    ///
    /// Mode 3 is the one entry whose behaviour is still taken on trust from
    /// kriomant/ch57x-keyboard-tool#173 rather than watched here, and its
    /// description says so.
    static let all: [LedMode] = [
        LedMode(
            id: "off", name: "Off",
            desc: "Unlit",
            icon: "power", number: 0,
            appearance: .unlit
        ),
        LedMode(
            id: "red", name: "Red",
            desc: "Steady red",
            icon: "circle.fill", number: 1,
            appearance: .steady(RGB(1.0, 0.15, 0.1))
        ),
        LedMode(
            id: "green", name: "Green",
            desc: "Steady green",
            icon: "circle.fill", number: 2,
            appearance: .steady(RGB(0.1, 1.0, 0.25))
        ),
        LedMode(
            id: "ripple", name: "Ripple",
            desc: "Pulses on input",
            icon: "waveform.path.ecg", number: 3,
            appearance: .pulse(RGB(0.25, 0.55, 1.0))
        ),
        LedMode(
            id: "rainbow", name: "Rainbow",
            desc: "Cycling colours — how the knob ships",
            icon: "rainbow", number: 4,
            appearance: .palette(RGB.vendorRainbow)
        ),
        LedMode(
            id: "rgb", name: "RGB",
            desc: "A second multicolour effect",
            icon: "sparkles", number: 5,
            appearance: .sweep
        )
    ]

    /// The colour to tint this mode's glyph, when it has one.
    ///
    /// Two modes share `circle.fill`, and what separates them is the only
    /// thing this device varies: red versus green. A shape alone cannot say
    /// that. The multicolour modes have no single colour, so they keep their
    /// own distinct symbols and no tint.
    var swatch: RGB? {
        switch appearance {
        case .steady(let rgb), .pulse(let rgb): return rgb
        case .unlit, .palette, .sweep: return nil
        }
    }

    /// Look up by wire name.
    static func named(_ id: String) -> LedMode? {
        all.first { $0.id == id }
    }

    /// Look up by the number `get_led` reports. `nil` for a number this
    /// build has no name for -- which is reported as unknown rather than
    /// guessed at, because a firmware with a mode we cannot name is exactly
    /// the case a fabricated label would hide.
    static func numbered(_ number: Int) -> LedMode? {
        all.first { $0.number == number }
    }

    /// How the firmware's reported mode number reads in the UI.
    static func describe(number: Int) -> String {
        guard let mode = numbered(number) else {
            return "Mode \(number) (unrecognised)"
        }
        return mode.name
    }
}

/// The mode the firmware reports holding after a write.
///
/// A named type rather than a bare `Int` because the number and its name are
/// one answer, and the pane compares it against what was asked for -- a
/// write that the device accepted and ignored comes back as the OLD mode,
/// which is only detectable if the reply carries a mode at all.
struct AppliedLedMode: Equatable, Sendable {
    let mode: Int

    var name: String { LedMode.describe(number: mode) }
}
