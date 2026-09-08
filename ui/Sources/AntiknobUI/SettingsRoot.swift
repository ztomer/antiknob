// SettingsRoot.swift — Main settings window root view.
// Native macOS TabView with system-managed tab bar and toolbar status cluster.

import AppKit
import SwiftUI

/// The window's tabs.
///
/// `layer(Int)` used to be here: one tab per host layer, so the strip grew
/// with the config and a five-layer setup pushed Hardware and Services off
/// the end. Layers live inside one Layers tab now, chosen by a control that
/// scales.
///
/// `lighting` was a tab of its own addressing the firmware's three DEVICE
/// layers. Lighting is per HOST layer -- the daemon writes the active
/// layer's mode on every switch -- so it belongs in the layer it describes,
/// not in a tab beside it.
/// The window's tabs.
///
/// `inspector` was one of these. Its three sections read the same device the
/// Hardware pane flashes, so they are sections of it now: a snoop, a slot
/// dump and a raw-packet console are things you reach for while looking at
/// the hardware, not a separate destination.
enum TabSelection: Hashable {
    case general
    case layers
    case hardware
    case services
}

public struct SettingsRoot: View {
    public init() {}

    @ObservedObject var store = ConfigStore.shared
    @State private var sel: TabSelection = .general

    public var body: some View {
        TabView(selection: $sel) {
            Tab("General", systemImage: "gearshape",
                value: TabSelection.general) {
                GeneralPane(store: store)
            }

            Tab("Layers", systemImage: "square.stack.3d.up",
                value: TabSelection.layers) {
                LayersPane(store: store)
            }

            Tab("Hardware", systemImage: "cpu",
                value: TabSelection.hardware) {
                HardwarePane(store: store)
            }

            Tab("Services", systemImage: "network",
                value: TabSelection.services) {
                ServicesPane(store: store)
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            StatusBar(store: store)
        }
        .frame(minWidth: 820, minHeight: 600)
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
            daemon
            separator
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

    /// Whether the daemon is up.
    ///
    /// The bar reported on the KNOB and said nothing about the process that
    /// drives it, so "connected knob, nothing happening" -- the state a
    /// stopped daemon produces -- looked exactly like a working setup. The
    /// Status & Diagnostics rows say the same thing, but only on one tab.
    private var daemon: some View {
        Image(systemName: store.daemonConnected
              ? "bolt.horizontal.circle.fill"
              : "bolt.horizontal.circle")
            .foregroundStyle(store.daemonConnected ? Color.green : Color.orange)
            .help(store.daemonConnected
                  ? "Daemon running (\(store.socketPath ?? "socket"))"
                  : "Daemon not running — changes are saved but not applied")
            .accessibilityLabel(store.daemonConnected
                                ? "Daemon running"
                                : "Daemon not running")
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
