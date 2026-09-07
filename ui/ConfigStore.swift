// ConfigStore.swift — Central reactive state store for Antiknob.
// Bridges the SwiftUI UI with the daemon socket and local config file.

import AppKit
import Foundation
import SwiftUI

final class ConfigStore: ObservableObject {
    static let shared = ConfigStore()

    @Published var cfg: Config = Config.defaultPOC {
        didSet {
            if !syncing {
                applyConfig(cfg)
                lastSaved = Date()
            }
        }
    }

    @Published var daemonConnected: Bool = false
    @Published var hardwareConnected: Bool = false
    @Published var hardwareProduct: String = "Not detected"
    @Published var lastSaved: Date?
    @Published var activeLayerIdx: Int = 0
    @Published var statusMessage: String?
    @Published var isBindingSlots: Bool = false

    private var syncing = false
    private let client = SocketClient.shared
    private var pollTimer: Timer?

    init() {
        loadInitial()
        startPolling()
    }

    deinit {
        pollTimer?.invalidate()
    }

    // MARK: - Initial Load and Polling

    func loadInitial() {
        syncing = true
        defer { syncing = false }

        // Attempt daemon load first
        if let config = try? client.getConfig() {
            self.cfg = config
            self.daemonConnected = true
        } else if let diskConfig = client.loadConfigFromDisk() {
            self.cfg = diskConfig
            self.daemonConnected = false
        } else {
            self.cfg = Config.defaultPOC
            self.daemonConnected = false
        }

        refreshStatus()
    }

    private func startPolling() {
        pollTimer = Timer.scheduledTimer(withTimeInterval: 2.5, repeats: true) { [weak self] _ in
            self?.refreshStatus()
        }
    }

    func refreshStatus() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self = self else { return }
            let connected = self.client.isConnected()
            var hwFound = false
            var product = "Not detected"

            if connected {
                if let status = try? self.client.getStatus(),
                   let devices = status["devices"] as? [[String: Any]], !devices.isEmpty {
                    hwFound = true
                    if let p = devices[0]["product_string"] as? String, !p.isEmpty {
                        product = p
                    } else {
                        product = "Anticater VK-01"
                    }
                }
            }

            DispatchQueue.main.async {
                self.daemonConnected = connected
                self.hardwareConnected = hwFound
                self.hardwareProduct = product
            }
        }
    }

    // MARK: - Config Persistence & Mutation

    func applyConfig(_ newConfig: Config) {
        let client = self.client
        let connected = self.daemonConnected
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                if connected {
                    try client.setConfig(newConfig)
                } else {
                    try client.saveConfigToDisk(newConfig)
                }
            } catch {
                // Fallback to disk if socket failed
                try? client.saveConfigToDisk(newConfig)
            }
        }
    }

    func reloadFromSource() {
        syncing = true
        defer { syncing = false }
        if let config = try? client.getConfig() {
            self.cfg = config
            self.daemonConnected = true
        } else if let diskConfig = client.loadConfigFromDisk() {
            self.cfg = diskConfig
        }
        refreshStatus()
    }

    // MARK: - Device Operations

    func setLed(layer: Int, mode: String, color: String?) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                let status = try self?.client.setLed(layer: layer, mode: mode, color: color)
                DispatchQueue.main.async {
                    self?.statusMessage = status ?? "LED updated"
                }
            } catch {
                DispatchQueue.main.async {
                    self?.statusMessage = "LED error: \(error.localizedDescription)"
                }
            }
        }
    }

    func bindSlots(layer: Int? = nil) {
        isBindingSlots = true
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                let status = try self?.client.bindSlots(layer: layer, dryRun: false)
                DispatchQueue.main.async {
                    self?.isBindingSlots = false
                    self?.statusMessage = status ?? "Slots bound to ⌃⌥F16..F20"
                }
            } catch {
                DispatchQueue.main.async {
                    self?.isBindingSlots = false
                    self?.statusMessage = "Bind error: \(error.localizedDescription)"
                }
            }
        }
    }
}
