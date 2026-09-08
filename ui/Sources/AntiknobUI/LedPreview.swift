// LedPreview.swift — what the knob's light does, drawn.
//
// Replaces a "16-LED RGB Ring Visualizer": sixteen addressable beads, a
// Solid Fill button, a Spectrum Gradient button. This device has none of
// that. It renders ONE ring of light whose colour the mode carries, it
// ignores the per-entry palette entirely (mode 1 was set with blue, red and
// green in turn and stayed red every time), and two of those three buttons
// only ever recoloured the beads on screen -- Spectrum Gradient did not even
// send a packet. A control that changes a picture of the hardware and not
// the hardware is worse than no control.
//
// So this draws the knob's single ring, animated as the selected mode
// actually behaves. The caption that used to sit under it -- naming the mode
// and flagging the one whose pattern was never captured -- is gone: the
// glyph row names the selection and the flash row says whether it is live,
// so the caption was a third statement of both.

import SwiftUI

extension RGB {
    /// The one place `RGB` becomes a `Color`. Kept here rather than on the
    /// model so `Core/` stays free of SwiftUI.
    var color: Color { Color(red: red, green: green, blue: blue) }
}

/// The lit ring, animated the way the given mode animates.
///
/// Was a whole second knob -- body, notch, ring -- drawn under the header's
/// knob in its own section, so a layer's pane showed the same object twice.
/// Only the ring was ever the point; it goes around the real one.
struct LedRing: View {
    let mode: LedMode

    /// Seconds per full cycle for the animated modes.
    private static let cyclePeriod: Double = 4.0
    private static let pulsePeriod: Double = 1.6

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: !animates)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            let colour = colour(at: t)
            let glow = glow(at: t)
            // Two passes: a soft bloom under a bright band. One stroke with
            // a shadow read as a dim indicator rather than as a lit knob.
            ZStack {
                Circle()
                    .strokeBorder(colour.opacity(0.55 * glow), lineWidth: 16)
                    .frame(width: 108, height: 108)
                    .blur(radius: 9)
                Circle()
                    .strokeBorder(colour.opacity(0.55 + 0.45 * glow), lineWidth: 8)
                    .frame(width: 104, height: 104)
                    .shadow(color: colour.opacity(0.9 * glow), radius: 14)
                    .shadow(color: colour.opacity(0.5 * glow), radius: 26)
            }
            .animation(.easeInOut(duration: 0.35), value: colour)
        }
        .allowsHitTesting(false)
        .accessibilityLabel("\(mode.name) lighting")
    }

    /// Only the effects need a clock. A steady colour redrawn thirty times a
    /// second is thirty times the work for the same pixels.
    private var animates: Bool {
        switch mode.appearance {
        case .unlit, .steady: return false
        case .pulse, .palette, .sweep: return true
        }
    }

    private func colour(at time: TimeInterval) -> Color {
        switch mode.appearance {
        case .unlit:
            return Color.secondary.opacity(0.25)
        case .steady(let rgb), .pulse(let rgb):
            return rgb.color
        case .palette(let entries):
            guard !entries.isEmpty else { return Color.secondary }
            let step = Int(time / (Self.cyclePeriod / Double(entries.count)))
            return entries[((step % entries.count) + entries.count) % entries.count].color
        case .sweep:
            let phase = time.truncatingRemainder(dividingBy: Self.cyclePeriod) / Self.cyclePeriod
            return Color(hue: phase, saturation: 0.9, brightness: 1.0)
        }
    }

    /// 0...1 brightness of the halo. Only `pulse` varies it.
    private func glow(at time: TimeInterval) -> Double {
        switch mode.appearance {
        case .unlit:
            return 0
        case .pulse:
            let phase = time.truncatingRemainder(dividingBy: Self.pulsePeriod) / Self.pulsePeriod
            return 0.25 + 0.75 * (0.5 - 0.5 * cos(phase * 2 * .pi))
        case .steady, .palette, .sweep:
            return 1
        }
    }
}
