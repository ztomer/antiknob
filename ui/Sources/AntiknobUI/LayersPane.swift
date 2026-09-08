// LayersPane.swift — every host layer, behind one tab.
//
// There used to be a tab per layer. The strip is in the window's titlebar
// beside the traffic lights, so it has a fixed budget: two layers fitted,
// five would have pushed Hardware and Services off the end. A tab bar inside
// one tab costs one row and does not care how many layers there are.
//
// Order, top to bottom: how you switch between layers, which layer you are
// editing, that layer's name and place, then the layer itself -- the knob
// with its light and gestures, and what those gestures do. Global settings
// first, then the selector, then the thing selected.

import SwiftUI

struct LayersPane: View {
    @ObservedObject var store: ConfigStore
    @State private var selected: Int = 0

    /// Never points past the end. Deleting the last layer, or a config
    /// reload that shortens the list, would otherwise index into nothing.
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

/// The layer tab bar: one pill per layer, then `+`.
///
/// Pills rather than a `Picker`, because a segmented control divides a fixed
/// width between however many segments it has -- six layers would shrink
/// every label to an ellipsis. These size to their own names and scroll when
/// the row runs out of room, so the tenth layer is as legible as the first.
struct LayerTabBar: View {
    @ObservedObject var store: ConfigStore
    @Binding var selected: Int
    let idx: Int

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(store.cfg.layers.indices, id: \.self) { i in
                    pill(i)
                }
                addButton
            }
            .padding(.vertical, 2)
        }
        // A scroll view takes all it is offered; without this the row would
        // claim the height of the pane it sits in.
        .frame(height: 30)
    }

    private func pill(_ i: Int) -> some View {
        let isSelected = i == idx
        return Button {
            selected = i
        } label: {
            Text(label(i))
                .font(.callout)
                .fontWeight(isSelected ? .medium : .regular)
                .foregroundStyle(isSelected ? Color.white : Color.primary)
                .lineLimit(1)
                .padding(.horizontal, 12)
                .padding(.vertical, 5)
                .background(
                    Capsule().fill(isSelected
                                   ? Color.accentColor
                                   : Color.secondary.opacity(0.14))
                )
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label(i))
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
        // Order is a real setting -- the hotkeys and double-tap cycle the
        // layers in this order -- so losing the row's chevrons could not
        // mean losing the ability to reorder. On the pill it is about the
        // one being pointed at.
        .contextMenu {
            Button("Move Left") { move(i, by: -1) }
                .disabled(i == 0)
            Button("Move Right") { move(i, by: 1) }
                .disabled(i >= store.cfg.layers.count - 1)
        }
    }

    private func move(_ i: Int, by delta: Int) {
        let dest = i + delta
        guard store.cfg.layers.indices.contains(i),
              store.cfg.layers.indices.contains(dest) else { return }
        withAnimation {
            store.cfg.layers.swapAt(i, dest)
            selected = dest
        }
    }

    private var addButton: some View {
        Button(action: addLayer) {
            Image(systemName: "plus")
                .font(.callout)
                .foregroundStyle(Color.secondary)
                // Same vertical padding as a pill's text, so the row sits on
                // one baseline instead of the + standing two points proud.
                .padding(.horizontal, 11)
                .padding(.vertical, 5)
                .background(Capsule().strokeBorder(Color.secondary.opacity(0.3), lineWidth: 1))
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help("Add a layer")
        .accessibilityLabel("Add a layer")
    }

    private func label(_ i: Int) -> String {
        let name = store[layer: i]?.name ?? ""
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

/// Rename, reorder, remove — one row under the tab bar.
///
/// Three rows before: a labelled name field, a labelled Order stepper with
/// an "n of m" readout, and a full-width destructive button. All three are
/// about the pill directly above them, and none needs a label of its own to
/// say so.
struct LayerEditRow: View {
    @ObservedObject var store: ConfigStore
    @Binding var selectedLayer: Int
    let idx: Int
    @State private var confirmingDelete = false

    var body: some View {
        HStack(spacing: 10) {
            // `.labelsHidden()`, or a Form turns the placeholder into a
            // leading label -- "Layer name" printed beside a field already
            // showing the layer's name, squeezing the field it labels.
            TextField("Layer name", text: store.layerName(idx))
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .frame(width: 200)

            Spacer(minLength: 8)

            Button(role: .destructive) { confirmingDelete = true } label: {
                Image(systemName: "trash")
            }
            .disabled(store.cfg.layers.count <= 1)
            .help(store.cfg.layers.count <= 1
                  ? "The last layer cannot be deleted"
                  : "Delete this layer")
            .confirmationDialog("Delete \(layerLabel)?", isPresented: $confirmingDelete) {
                Button("Delete Layer", role: .destructive) { delete() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Its gesture bindings are removed. This cannot be undone.")
            }
        }
    }

    private var layerLabel: String {
        let name = store[layer: idx]?.name ?? ""
        return name.isEmpty ? "Layer \(idx + 1)" : name
    }

    private func delete() {
        guard store.cfg.layers.count > 1,
              store.cfg.layers.indices.contains(idx) else { return }
        withAnimation {
            store.cfg.layers.remove(at: idx)
            selectedLayer = min(idx, store.cfg.layers.count - 1)
        }
    }
}
