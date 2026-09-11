// App.swift — Antiknob's `@main` entry point.
//
// Deliberately thin: `@main` cannot live in a library, so this target
// holds the scene graph and nothing else. Everything it composes --
// the delegate, the menu-bar content, the settings root -- lives in
// AntiknobUI, where tests can reach it.

import SwiftUI

import AntiknobUI

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
            Image(nsImage: MenuBarIcon.iconImage)
        }
        .menuBarExtraStyle(.menu)
    }
}
