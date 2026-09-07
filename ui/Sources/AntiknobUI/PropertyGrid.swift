// PropertyGrid.swift — left-aligned property and record tables.
//
// SwiftUI's `LabeledContent` in a grouped Form pushes its value to the
// trailing edge. That is right for a lone scalar, but a column of them (or
// worse, a column of multi-part records) ends up with every value starting at
// a different x, and nothing for the eye to read down. `Grid` sizes each
// column to its widest cell, so these give every row the same column edges
// with the text left-aligned inside each one.

import SwiftUI

/// A table of rows sharing one set of column edges.
///
/// Put `PropertyRow`s (or bare `GridRow`s) inside. Lives in a single Form
/// `Section` row rather than one Form row per entry, because the columns can
/// only line up if the rows are laid out together.
struct PropertyGrid<Content: View>: View {
    var horizontalSpacing: CGFloat = 16
    var verticalSpacing: CGFloat = 7
    @ViewBuilder var content: Content

    var body: some View {
        Grid(
            alignment: .leadingFirstTextBaseline,
            horizontalSpacing: horizontalSpacing,
            verticalSpacing: verticalSpacing
        ) {
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 2)
    }
}

/// One `label: value` row. The label column is sized by the longest label in
/// the grid, so values start at a single x for the whole table.
struct PropertyRow<Value: View>: View {
    let label: String
    @ViewBuilder var value: Value

    var body: some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
                .gridColumnAlignment(.leading)
            value
                .gridColumnAlignment(.leading)
        }
    }
}

/// A short uppercase tag, used for transports. Sized by its grid column
/// rather than by its own text, so tags of different lengths still leave the
/// following column on a straight edge.
struct TagPill: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: 9, weight: .bold))
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(Capsule().fill(Color.secondary.opacity(0.15)))
    }
}
