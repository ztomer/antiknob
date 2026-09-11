// MenuBar.swift — App lifecycle delegate and the MenuBarExtra (AK12) content.
//
// Lives in the library rather than beside `@main` so it is reachable from
// `@testable import AntiknobUI`; the executable target holds the scene only.

import AppKit
import SwiftUI

@MainActor
public final class AppDelegate: NSObject, NSApplicationDelegate {
    public override init() { super.init() }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        Task { @MainActor in
            if let window = NSApp.windows.first {
                if let zoom = window.standardWindowButton(.zoomButton) {
                    zoom.isHidden = true
                    zoom.isEnabled = false
                }
                window.center()
            }
        }
    }

    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}

// MARK: - Menu Bar Icon & Menu

/// The knob's mark, drawn for the menu bar: ring plus pointer dot.
///
/// Strokes rather than pixels, legible wherever it lands, and a template
/// citizen in the menu bar -- light and dark just work.
struct KnobMark: View {
    var body: some View {
        ZStack {
            Circle().strokeBorder(lineWidth: 2)
            Circle().fill().frame(width: 3, height: 3).offset(y: -4.5)
        }
        .frame(width: 14, height: 14)
    }
}

public struct MenuBarIcon: View {
    public init() {}

    public var body: some View {
        // Primary, not a fixed ink: the menu bar is dark or light and the
        // mark must read on both. State lives in the menu itself, not here.
        KnobMark()
            .foregroundStyle(.primary)
    }
}

public struct MenuBarView: View {
    public init() {}

    @ObservedObject var store = ConfigStore.shared
    @Environment(\.openWindow) private var openWindow

    public var body: some View {
        Text("Antiknob")
            .font(.headline)

        Text("\(store.hardwareConnected ? store.hardwareProduct : "No Device Detected") • \(store.transportDisplay)")

        Divider()

        ForEach(Array(store.cfg.layers.enumerated()), id: \.offset) { idx, layer in
            Button {
                store.setActiveLayer(idx)
            } label: {
                if store.activeLayerIdx == idx {
                    Text("✓ \(layer.name.isEmpty ? "Layer \(idx + 1)" : layer.name)")
                } else {
                    Text("   \(layer.name.isEmpty ? "Layer \(idx + 1)" : layer.name)")
                }
            }
        }

        Divider()

        Toggle("Start at Login", isOn: Binding(
            get: { store.startOnLogin },
            set: { store.toggleStartOnLogin(enabled: $0) }
        ))

        Divider()

        Button("Settings…") {
            NSApp.activate(ignoringOtherApps: true)
            let isSettings: (NSWindow) -> Bool = { window in
                window.title == "Antiknob Settings" || window.identifier?.rawValue == "main"
            }
            if let window = NSApp.windows.first(where: isSettings) {
                window.makeKeyAndOrderFront(nil)
            } else {
                openWindow(id: "main")
            }
        }
        .keyboardShortcut(",", modifiers: .command)

        Button("Reload Configuration") {
            store.reloadFromSource()
        }
        .keyboardShortcut("r", modifiers: .command)

        Divider()

        Button("Quit Antiknob") {
            NSApp.terminate(nil)
        }
        .keyboardShortcut("q", modifiers: .command)
    }
}
