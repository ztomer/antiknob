// KnobHeader.swift — the knob, lit, with its five gestures around it.
//
// One picture of the knob per pane. The lighting section used to draw a
// second one directly below this, so the pane showed the same knob twice --
// once with clickable gesture zones and no light, once with the light and
// nothing to click. They are the same object, so this one is lit.
//
// That also puts the colour where it belongs: the ring is what the layer's
// mode does, drawn around the gestures the layer binds, in the layer that
// owns both.

import SwiftUI

struct KnobHeader: View {
    @Binding var selected: Gesture?
    /// The mode this layer puts on the knob, if it sets one. `nil` draws the
    /// knob unlit -- which is what "leave the backlight alone" looks like,
    /// and must not be confused with mode `off`.
    var mode: LedMode?

    var body: some View {
        HStack(alignment: .center, spacing: 20) {
            VStack(spacing: 12) {
                zone(.twistL, "arrow.counterclockwise", "Twist Left")
                zone(.holdTwistL, "arrow.counterclockwise.circle", "Hold + Twist L")
            }
            knobBody
            VStack(spacing: 12) {
                zone(.twistR, "arrow.clockwise", "Twist Right")
                zone(.holdTwistR, "arrow.clockwise.circle", "Hold + Twist R")
            }
        }
        .padding(.vertical, 8)
    }

    private var knobBody: some View {
        // Clear of the ring. The label sat 6pt under an 84pt knob, which was
        // fine until the knob grew a 108pt lit ring around it -- "Press"
        // then printed over the bottom of the glow.
        VStack(spacing: 18) {
            ZStack {
                // The lit ring, when this layer sets a mode. Sized to sit
                // just outside the knob body so the two read as one object.
                if let mode {
                    LedRing(mode: mode)
                }

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
                    .shadow(color: Color.black.opacity(0.18), radius: 5, x: 0, y: 3)

                // Indicator notch
                Capsule()
                    .fill(Color.secondary.opacity(0.6))
                    .frame(width: 4, height: 14)
                    .offset(y: -26)
            }
            .frame(width: 84, height: 84)
            .overlay(
                Circle()
                    .strokeBorder(Color.accentColor, lineWidth: selected == .press ? 2.5 : 0)
            )
            .contentShape(Circle())
            .onTapGesture { toggle(.press) }

            Text("Press")
                .font(.caption2)
                .fontWeight(selected == .press ? .semibold : .regular)
                .foregroundStyle(selected == .press ? Color.accentColor : Color.secondary)
        }
        // Room for the ring, so the knob does not shift when a mode is set.
        .frame(width: 116)
    }

    private func zone(_ g: Gesture, _ symbol: String, _ label: String) -> some View {
        Button { toggle(g) } label: {
            VStack(spacing: 4) {
                Image(systemName: symbol)
                    .font(.title3)
                Text(label)
                    .font(.caption2)
                    .lineLimit(1)
            }
            .frame(width: 96, height: 48)
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .foregroundStyle(selected == g ? Color.accentColor : Color.secondary)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(selected == g ? Color.accentColor.opacity(0.14) : Color.clear)
        )
    }

    private func toggle(_ g: Gesture) {
        selected = (selected == g ? nil : g)
    }
}
