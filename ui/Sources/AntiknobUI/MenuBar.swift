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
/// Rendered as a vector template NSImage so AppKit status items and SwiftUI
/// views display it crisply in both light and dark menu bar contexts.
public struct MenuBarIcon: View {
    public init() {}

    public static let iconImage: NSImage = {
        let size = NSSize(width: 18, height: 18)
        let img = NSImage(size: size, flipped: false) { _ in
            let circleRect = NSRect(x: 2.0, y: 2.0, width: 14.0, height: 14.0)
            let path = NSBezierPath(ovalIn: circleRect)
            path.lineWidth = 1.8
            NSColor.black.setStroke()
            path.stroke()

            let dotRect = NSRect(x: 7.5, y: 11.5, width: 3.0, height: 3.0)
            let dotPath = NSBezierPath(ovalIn: dotRect)
            NSColor.black.setFill()
            dotPath.fill()

            return true
        }
        img.isTemplate = true
        return img
    }()

    public var body: some View {
        Image(nsImage: Self.iconImage)
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
