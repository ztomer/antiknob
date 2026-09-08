// Layout.swift — shared control metrics.
//
// One definition per dimension, so the same control is the same size on every
// pane. Before this, the action dropdown was 79, 103, 108, 114, 120, 125,
// 131, 134, 148 and 151 points wide depending on which label it happened to
// be showing, and started at x=213, 227, 253 or 385 depending on the pane --
// the same control, ten sizes, four left edges. A menu that resizes with its
// own current value also moves under the pointer when you change it.
//
// These are measured, not guessed: `LayoutTests` renders every label that can
// appear in a dropdown in the real system font and fails if one would not fit.

import SwiftUI

enum Layout {
    /// Every pop-up / menu control in the app.
    ///
    /// Uniform width does two things a natural-width menu cannot: the
    /// disclosure chevrons line up down a pane, and the control keeps its
    /// size and position when its selection changes.
    static let dropdown: CGFloat = 216

    /// Chrome `Dropdown` draws around its label: 9pt horizontal padding each
    /// side, the chevron, and the gap before it. Subtracted from `dropdown`
    /// when checking a label fits, with a little slack so the check errs
    /// toward demanding room rather than toward truncation.
    static let dropdownChrome: CGFloat = 44

    /// Label column beside a pop-up, so the pop-ups on Hardware and Inspector
    /// start at the same x rather than at whatever their own label's length
    /// happens to be. Sized for "Memory Group" (90.3pt measured).
    static let controlLabel: CGFloat = 100
}

/// The app's one dropdown control.
///
/// The label is drawn here rather than handed to a `Picker`, because a
/// stock macOS menu control is an NSPopUpButton that sizes itself to its
/// current title and IGNORES the frame it is given -- measured three ways:
/// the same `.frame(width:)` produced 216pt in one container and 142pt in
/// another; widening that frame to 400pt moved the control sideways without
/// resizing it; and sizing the label instead changed nothing. The result was
/// the same control rendered at 79, 114, 131, 142, 148, 151 and 179 points
/// across the app, with its chevron wherever the current value happened to
/// end.
///
/// `.borderlessButton` style with the native indicator hidden makes SwiftUI
/// render THIS view as the label, so the width is ours to set. The chevron is
/// drawn explicitly for the same reason.
struct Dropdown<Content: View>: View {
    let title: String
    @ViewBuilder var content: Content

    var body: some View {
        Menu {
            content
        } label: {
            HStack(spacing: 6) {
                Text(title)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 4)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .frame(width: Layout.dropdown, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color.secondary.opacity(0.16))
            )
            .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
    }
}
