// LayersPane.swift — every host layer, behind one tab.
//
// There used to be a tab per layer. The strip is in the window's titlebar
// beside the traffic lights, so it has a fixed budget: two layers fitted,
// five would have pushed Hardware and Services off the end, and the `+` in
// the toolbar made growing past that a normal thing to do. A chooser inside
// one tab costs one row and does not care how many layers there are.
//
// The `+` moved here with them, next to the thing it adds.

import SwiftUI

struct LayersPane: View {
    @ObservedObject var store: ConfigStore
    @State private var selected: Int = 0

    /// Never points past the end. Deleting the last layer, or a config
    /// reload that shortens the list, would otherwise leave this indexing
    /// into nothing.
    private var idx: Int {
        min(selected, max(store.cfg.layers.count - 1, 0))
    }

    var body: some View {
        Group {
            if store.cfg.layers.isEmpty {
                ContentUnavailableView(
                    "No layers",
                    systemImage: "square.stack.3d.up",
                    description: Text("Add one to bind the knob's gestures.")
                )
            } else {
                LayerDetail(store: store, idx: idx, selectedLayer: $selected)
                    .id(idx)
            }
        }
        .onChange(of: store.cfg.layers.count) { _, count in
            selected = min(selected, max(count - 1, 0))
        }
    }
}

/// The chooser row: which layer, and one more.
///
/// A `Dropdown` rather than a segmented control because the list grows. Six
/// segments of "Layer 6" is a control that shrinks its own labels to
/// illegibility; a pop-up is the same width whatever it holds.
struct LayerChooser: View {
    @ObservedObject var store: ConfigStore
    @Binding var selected: Int
    let idx: Int

    var body: some View {
        GridRow {
            Text("Layer")
                .foregroundStyle(.secondary)
                .gridColumnAlignment(.leading)

            HStack(spacing: 8) {
                Dropdown(title: label(idx)) {
                    Picker("", selection: $selected) {
                        ForEach(store.cfg.layers.indices, id: \.self) { i in
                            Text(label(i)).tag(i)
                        }
                    }
                    .pickerStyle(.inline).labelsHidden()
                }

                Button(action: addLayer) {
                    Image(systemName: "plus")
                }
                .help("Add a layer")
            }
            .gridColumnAlignment(.leading)
        }
    }

    private func label(_ i: Int) -> String {
        guard store.cfg.layers.indices.contains(i) else { return "" }
        let name = store.cfg.layers[i].name
        return name.isEmpty ? "Layer \(i + 1)" : name
    }

    private func addLayer() {
        withAnimation {
            store.cfg.layers.append(
                LayerConfig(name: "Layer \(store.cfg.layers.count + 1)")
            )
            selected = store.cfg.layers.count - 1
        }
    }
}
