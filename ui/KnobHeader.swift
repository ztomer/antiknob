// KnobHeader.swift — Skeuomorphic interactive knob centerpiece.
// Clickable gesture zones highlight corresponding rows in the settings form.

import SwiftUI

struct KnobHeader: View {
    @Binding var selected: Gesture?

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
        VStack(spacing: 6) {
            ZStack {
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
