// SettingsRoot.swift — Main settings window root view.
// Features horizontal layer tab strip with drag-to-reorder, transient Saved badge,
// and grouped detail view with native Liquid Glass macOS design.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

enum TabSelection: Hashable {
    case switching
    case layer(Int)
    case lighting
}

struct SettingsRoot: View {
    @ObservedObject var store = ConfigStore.shared
    @State private var sel: TabSelection = .switching

    var body: some View {
        VStack(spacing: 0) {
            TabStrip(store: store, sel: $sel)
            Divider()
            Group {
                switch sel {
                case .layer(let i) where i < store.cfg.layers.count:
                    LayerDetail(store: store, idx: i).id(i)
                case .lighting:
                    LedSection(store: store)
                default:
                    GeneralPane(store: store)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 660, minHeight: 580)
        .onChange(of: store.cfg.layers.count) { _, newCount in
            if case .layer(let i) = sel, i >= newCount {
                sel = newCount > 0 ? .layer(newCount - 1) : .switching
            }
        }
    }
}

// MARK: - Tab Strip

struct TabStrip: View {
    @ObservedObject var store: ConfigStore
    @Binding var sel: TabSelection
    @State private var dragging: Int?

    var body: some View {
        HStack(spacing: 4) {
            Spacer(minLength: 0)

            chip(.switching) {
                Label("Switching", systemImage: "arrow.triangle.2.circlepath")
            }

            Divider()
                .frame(height: 16)
                .padding(.horizontal, 4)

            ForEach(Array(store.cfg.layers.enumerated()), id: \.offset) { i, l in
                chip(.layer(i)) {
                    Text(l.name.isEmpty ? "Layer \(i + 1)" : l.name)
                }
                .onDrag {
                    dragging = i
                    return NSItemProvider(object: "antiknob-layer-\(i)" as NSString)
                }
                .onDrop(of: [.text], delegate: LayerDropDelegate(
                    target: i,
                    dragging: $dragging,
                    store: store,
                    sel: $sel
                ))
                .contextMenu {
                    Button("Move Left") { move(i, by: -1) }
                        .disabled(i == 0)
                    Button("Move Right") { move(i, by: 1) }
                        .disabled(i == store.cfg.layers.count - 1)
                    Divider()
                    Button("Delete Layer", role: .destructive) { remove(i) }
                        .disabled(store.cfg.layers.count <= 1)
                }
            }

            Button(action: addLayer) {
                Image(systemName: "plus")
            }
            .buttonStyle(.borderless)
            .help("Add a layer")

            Divider()
                .frame(height: 16)
                .padding(.horizontal, 4)

            chip(.lighting) {
                Label("Lighting", systemImage: "lightbulb.fill")
            }

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(.bar)
        .overlay(alignment: .trailing) {
            SavedBadge(store: store)
                .padding(.trailing, 14)
        }
    }

    private func chip(_ tag: TabSelection, @ViewBuilder label: () -> some View) -> some View {
        Button {
            sel = tag
        } label: {
            label()
                .padding(.horizontal, 11)
                .padding(.vertical, 5)
                .background(Capsule().fill(.quaternary).opacity(sel == tag ? 1 : 0))
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .foregroundStyle(sel == tag ? .primary : .secondary)
    }

    private func addLayer() {
        withAnimation {
            let nextNum = store.cfg.layers.count + 1
            store.cfg.layers.append(LayerConfig(
                name: "Layer \(nextNum)",
                twistL: nil, twistR: nil,
                holdTwistL: nil, holdTwistR: nil, press: nil
            ))
            sel = .layer(store.cfg.layers.count - 1)
        }
    }

    private func move(_ i: Int, by delta: Int) {
        let j = i + delta
        guard store.cfg.layers.indices.contains(j) else { return }
        withAnimation {
            store.cfg.layers.swapAt(i, j)
            if sel == .layer(i) { sel = .layer(j) }
            else if sel == .layer(j) { sel = .layer(i) }
        }
    }

    private func remove(_ i: Int) {
        guard store.cfg.layers.count > 1 else { return }
        withAnimation {
            store.cfg.layers.remove(at: i)
            if case .layer(let s) = sel {
                if s == i { sel = .layer(min(i, store.cfg.layers.count - 1)) }
                else if s > i { sel = .layer(s - 1) }
            }
        }
    }
}

// MARK: - Drag and Drop Delegate

struct LayerDropDelegate: DropDelegate {
    let target: Int
    @Binding var dragging: Int?
    let store: ConfigStore
    @Binding var sel: TabSelection

    func dropEntered(info: DropInfo) {
        guard let from = dragging, from != target,
              store.cfg.layers.indices.contains(from),
              store.cfg.layers.indices.contains(target) else { return }

        withAnimation {
            store.cfg.layers.move(
                fromOffsets: IndexSet(integer: from),
                toOffset: target > from ? target + 1 : target
            )
            if case .layer(let s) = sel {
                if s == from { sel = .layer(target) }
                else if from < target, s > from, s <= target { sel = .layer(s - 1) }
                else if target < from, s >= target, s < from { sel = .layer(s + 1) }
            }
        }
        dragging = target
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        dragging = nil
        return true
    }
}

// MARK: - Saved Badge

struct SavedBadge: View {
    @ObservedObject var store: ConfigStore
    @State private var visible = false
    @State private var hideItem: DispatchWorkItem?

    var body: some View {
        Label("Saved", systemImage: "checkmark.circle.fill")
            .font(.caption)
            .foregroundStyle(.secondary)
            .opacity(visible ? 1 : 0)
            .accessibilityHidden(!visible)
            .onChange(of: store.lastSaved) { _, saved in
                guard saved != nil else { return }
                withAnimation(.easeIn(duration: 0.12)) { visible = true }
                hideItem?.cancel()
                let task = DispatchWorkItem {
                    withAnimation(.easeOut(duration: 0.5)) { visible = false }
                }
                hideItem = task
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.3, execute: task)
            }
    }
}
