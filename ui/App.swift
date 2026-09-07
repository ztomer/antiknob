// App.swift — Main application entry point for Antiknob.
// Configures the native macOS window, menu items, MenuBarExtra (AK12), and app lifecycle.

import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        Task { @MainActor in
            if let window = NSApp.windows.first {
                window.title = "Antiknob Settings"
                window.titleVisibility = .hidden
                window.titlebarAppearsTransparent = true
                window.styleMask.insert(.fullSizeContentView)
                window.isMovableByWindowBackground = true
                if let zoom = window.standardWindowButton(.zoomButton) {
                    zoom.isHidden = true
                    zoom.isEnabled = false
                }
                window.center()
            }
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}

// MARK: - Menu Bar Icon & Menu

struct MenuBarIcon: View {
    private var iconImage: NSImage {
        if let url = Bundle.main.url(forResource: "ak12-tray@2x", withExtension: "png"),
           let img = NSImage(contentsOf: url) {
            img.size = NSSize(width: 18, height: 18)
            return img
        }
        if let url = Bundle.main.url(forResource: "ak12-tray", withExtension: "png"),
           let img = NSImage(contentsOf: url) {
            img.size = NSSize(width: 18, height: 18)
            return img
        }
        if let url = Bundle.main.url(forResource: "ak12-1024", withExtension: "png"),
           let img = NSImage(contentsOf: url) {
            img.size = NSSize(width: 18, height: 18)
            return img
        }
        let fallback = NSImage(systemSymbolName: "dial.low.fill", accessibilityDescription: "Antiknob") ?? NSImage()
        fallback.size = NSSize(width: 18, height: 18)
        return fallback
    }

    var body: some View {
        Image(nsImage: iconImage)
    }
}

struct MenuBarView: View {
    @ObservedObject var store = ConfigStore.shared
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Text("Antiknob")
            .font(.headline)

        Text("\(store.hardwareConnected ? store.hardwareProduct : "No Device Detected") • \(store.transportDisplay)")

        Divider()

        ForEach(Array(store.cfg.layers.enumerated()), id: \.offset) { idx, layer in
            Button {
                store.activeLayerIdx = idx
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
            if let window = NSApp.windows.first(where: { $0.title == "Antiknob Settings" || $0.identifier?.rawValue == "main" }) {
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

// MARK: - App Scene

@main
struct AntiknobApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        Window("Antiknob Settings", id: "main") {
            SettingsRoot()
        }
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(after: .appInfo) {
                Button("Reload Configuration") {
                    ConfigStore.shared.reloadFromSource()
                }
                .keyboardShortcut("r", modifiers: .command)

                Divider()

                Button("Bind Slots to Firmware") {
                    ConfigStore.shared.bindSlots()
                }
            }
        }

        MenuBarExtra {
            MenuBarView()
        } label: {
            MenuBarIcon()
        }
        .menuBarExtraStyle(.menu)
    }
}
