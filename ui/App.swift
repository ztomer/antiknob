// App.swift — Main application entry point for Antiknob.
// Configures the native macOS window, menu items, and app lifecycle.

import AppKit
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)

        if let window = NSApp.windows.first {
            window.title = "Antiknob Settings"
            window.center()
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}

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
    }
}
