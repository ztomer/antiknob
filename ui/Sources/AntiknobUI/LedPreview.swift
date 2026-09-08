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
// actually behaves, and says when it is approximating.

import SwiftUI

extension RGB {
    /// The one place `RGB` becomes a `Color`. Kept here rather than on the
    /// model so `Core/` stays free of SwiftUI.
    var color: Color { Color(red: red, green: green, blue: blue) }
}

/// The knob, lit the way the given mode lights it.
///
/// `isLive` says whether this is what the hardware is doing right now or a
/// preview of a selection not yet sent. The two are drawn differently on
/// purpose: an unsent selection is an intention, and an intention rendered
/// identically to a fact is the defect this app keeps finding in itself.
struct LedPreview: View {
    let mode: LedMode
    var isLive: Bool = false

    /// Seconds per full cycle for the animated modes.
    private static let cyclePeriod: Double = 4.0
    private static let pulsePeriod: Double = 1.6

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: !animates)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            ring(color: colour(at: t), glow: glow(at: t))
        }
        .frame(width: 132, height: 132)
        .accessibilityLabel("\(mode.name) preview")
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

    private func ring(color: Color, glow: Double) -> some View {
        ZStack {
            // The lit ring: one band of colour, which is all this device has.
            Circle()
                .strokeBorder(color.opacity(0.25 + 0.75 * glow), lineWidth: 9)
                .frame(width: 108, height: 108)
                .shadow(color: color.opacity(0.75 * glow), radius: 12)

            // The knob body sitting inside it.
            Circle()
                .fill(LinearGradient(
                    colors: [
                        Color(nsColor: .controlBackgroundColor),
                        Color(nsColor: .windowBackgroundColor)
                    ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ))
                .overlay(Circle().strokeBorder(.separator, lineWidth: 1))
                .frame(width: 84, height: 84)

            Capsule()
                .fill(Color.secondary.opacity(0.6))
                .frame(width: 4, height: 13)
                .offset(y: -25)
        }
        .animation(.easeInOut(duration: 0.35), value: color)
    }
}

/// The caption under the preview: what it is showing, and how much of it is
/// a reproduction rather than a stand-in.
struct LedPreviewCaption: View {
    let mode: LedMode
    let isLive: Bool

    var body: some View {
        VStack(spacing: 3) {
            Text(isLive
                 ? "\(mode.name) — on the knob now"
                 : "\(mode.name) — not sent yet")
                .font(.caption)
                .foregroundStyle(isLive ? Color.primary : Color.secondary)

            if mode.appearance.isApproximate {
                Text("Preview approximate: this mode's pattern has not been captured off the wire.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
