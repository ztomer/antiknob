// SettingsRoot.swift — Main settings window root view.
// Native macOS TabView with system-managed tab bar and toolbar status cluster.

import AppKit
import SwiftUI

enum TabSelection: Hashable {
    case switching
    case layer(Int)
    case lighting
    case hardware
    case inspector
    case services
}

struct SettingsRoot: View {
    @ObservedObject var store = ConfigStore.shared
    @State private var sel: TabSelection = .switching

    var body: some View {
        TabView(selection: $sel) {
            Tab("Switching", systemImage: "arrow.triangle.2.circlepath",
                value: TabSelection.switching) {
                GeneralPane(store: store)
            }

            ForEach(Array(store.cfg.layers.enumerated()), id: \.offset) { i, layer in
                Tab(layer.name.isEmpty ? "Layer \(i + 1)" : layer.name,
                    systemImage: "square.stack.3d.up",
                    value: TabSelection.layer(i)) {
                    LayerDetail(store: store, idx: i, sel: $sel).id(i)
                }
            }

            Tab("Lighting", systemImage: "lightbulb.fill",
                value: TabSelection.lighting) {
                LedSection(store: store)
            }

            Tab("Hardware", systemImage: "cpu",
                value: TabSelection.hardware) {
                HardwarePane(store: store)
            }

            Tab("Inspector", systemImage: "waveform.path.ecg",
                value: TabSelection.inspector) {
                InspectorPane(store: store)
            }

            Tab("Services", systemImage: "network",
                value: TabSelection.services) {
                ServicesPane(store: store)
            }
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button(action: addLayer) {
                    Image(systemName: "plus")
                }
                .help("Add a layer")
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            StatusBar(store: store)
        }
        .frame(minWidth: 820, minHeight: 600)
        .onChange(of: store.cfg.layers.count) { _, newCount in
            if case .layer(let i) = sel, i >= newCount {
                sel = newCount > 0 ? .layer(newCount - 1) : .switching
            }
        }
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
}

// MARK: - Status Bar

/// Bottom-trailing status cluster: connection, transport, power source, and a
/// manual refresh. Each state reads as a glyph -- the words live in the
/// tooltip and the accessibility label, so the bar stays quiet until hovered.
struct StatusBar: View {
    @ObservedObject var store: ConfigStore

    var body: some View {
        HStack(spacing: 8) {
            SavedBadge(store: store)
            Spacer(minLength: 0)
            connection
            separator
            transport
            separator
            power
            Button { store.refreshStatus() } label: {
                Image(systemName: "arrow.clockwise")
                    .contentShape(Rectangle())
                    .frame(width: 22, height: 18)
            }
            .buttonStyle(.borderless)
            .help("Refresh diagnostics")
            .accessibilityLabel("Refresh diagnostics")
        }
        .font(.callout)
        .imageScale(.medium)
        .padding(.leading, 12)
        .padding(.trailing, 8)
        .padding(.vertical, 4)
        .frame(height: 28)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }

    private var separator: some View {
        Divider().frame(height: 13)
    }

    private var connection: some View {
        Circle()
            .fill(store.hardwareConnected
                  ? store.transportColor
                  : Color.secondary.opacity(0.4))
            .frame(width: 8, height: 8)
            .help(store.hardwareConnected
                  ? store.hardwareProduct
                  : "No device detected")
            .accessibilityLabel(store.hardwareConnected
                                ? "Connected to \(store.hardwareProduct)"
                                : "No device detected")
    }

    private var transport: some View {
        Image(systemName: store.transportIcon)
            .foregroundStyle(store.hardwareConnected
                             ? store.transportColor
                             : Color.secondary)
            .help(store.transportDisplay)
            .accessibilityLabel(store.transportDisplay)
    }

    private var power: some View {
        Image(systemName: store.powerIcon)
            .foregroundStyle(.secondary)
            .help(store.hardwareConnected
                  ? store.powerDescription
                  : "No power source")
            .accessibilityLabel(store.hardwareConnected
                                ? store.powerDescription
                                : "No power source")
    }
}

// MARK: - Saved Badge

struct SavedBadge: View {
    @ObservedObject var store: ConfigStore
    @State private var visible = false
    @State private var hideTask: Task<Void, Never>?

    var body: some View {
        Label("Saved", systemImage: "checkmark.circle.fill")
            .font(.caption)
            .imageScale(.small)
            .foregroundStyle(.secondary)
            .opacity(visible ? 1 : 0)
            .accessibilityHidden(!visible)
            .onChange(of: store.lastSaved) { _, saved in
                guard saved != nil else { return }
                withAnimation(.easeIn(duration: 0.12)) { visible = true }
                hideTask?.cancel()
                hideTask = Task { @MainActor in
                    try? await Task.sleep(for: .seconds(1.3))
                    guard !Task.isCancelled else { return }
                    withAnimation(.easeOut(duration: 0.5)) { visible = false }
                }
            }
    }
}
