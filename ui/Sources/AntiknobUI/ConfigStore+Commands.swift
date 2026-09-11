// ConfigStore+Commands.swift — the store's device commands.
//
// Six methods with one shape: hop off the main actor, call the daemon over
// the socket, hop back and report. They are grouped here rather than left in
// the store's body because they share that shape and nothing else -- none of
// them touches the store's config state, and reading `ConfigStore` to
// understand how config is loaded and published should not mean scrolling
// past a hundred lines of one-shot RPCs.
//
// This also brings the type body back under the lint cap it had been over
// since before the cap was introduced, without any of the six changing.

import Foundation

extension ConfigStore {
    /// Write a layer's backlight mode and report what the firmware holds
    /// afterwards.
    ///
    /// The completion carries the daemon's READ-BACK, not the fact that a
    /// packet was sent. This call used to read a `status` key the daemon has
    /// never sent, so its `?? "OK"` fallback fired every single time -- the
    /// pane displayed a success string the app made up, for a write nothing
    /// had confirmed. On a `514c:8850` that is precisely the failure mode:
    /// without its init packet the firmware accepts an LED write, stores it,
    /// reads it back, and changes nothing.
    func setLed(
        layer: Int,
        mode: String,
        completion: @escaping @MainActor @Sendable (Result<AppliedLedMode, Error>) -> Void
    ) {
        Task.detached(priority: .userInitiated) {
            do {
                let applied = try SocketClient.shared.setLed(layer: layer, mode: mode)
                await MainActor.run { [weak self] in
                    self?.statusMessage = "Layer \(layer + 1) backlight: \(applied.name)"
                    completion(.success(applied))
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.statusMessage = "LED error: \(error.localizedDescription)"
                    completion(.failure(error))
                }
            }
        }
    }

    /// Switch the daemon's active host layer.
    ///
    /// The menu bar's layer list used to assign `store.activeLayerIdx` and
    /// stop there -- a `@Published` value nothing else read and nothing ever
    /// sent. Picking a layer moved a checkmark and changed no behaviour, on
    /// a menu whose entire purpose is switching layers. `SocketClient.setLayer`
    /// existed the whole time and had no callers.
    func setActiveLayer(_ index: Int) {
        let previous = activeLayerIdx
        // Moved at once so the menu answers the click, then corrected by the
        // daemon's reply -- or put back if it refuses.
        activeLayerIdx = index
        Task.detached(priority: .userInitiated) {
            do {
                try SocketClient.shared.setLayer(index)
                await MainActor.run { [weak self] in self?.refreshStatus() }
            } catch {
                await MainActor.run { [weak self] in
                    self?.activeLayerIdx = previous
                    self?.statusMessage = "Layer switch failed: \(error.localizedDescription)"
                }
            }
        }
    }

    public func bindSlots(layer: Int? = nil) {
        isBindingSlots = true
        Task.detached(priority: .userInitiated) {
            do {
                let status = try SocketClient.shared.bindSlots(layer: layer)
                await MainActor.run { [weak self] in
                    guard let self else { return }
                    self.isBindingSlots = false
                    // Say when the light cannot have followed: the daemon
                    // syncs the active host layer's mode after a flash, and
                    // a layer that names no mode implies no write. Without
                    // this a successful flash with an unchanged light reads
                    // as a failed one.
                    var msg = status
                    let led = self.cfg.layers.indices.contains(self.activeLayerIdx)
                        ? self.cfg.layers[self.activeLayerIdx].led?
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                        : nil
                    if led == nil || led!.isEmpty {
                        msg += " — active layer names no backlight mode, light unchanged"
                    }
                    self.statusMessage = msg
                    // The knob's mode changed underneath the layer panes;
                    // they must not keep drawing the old answer. Same as
                    // flashKeymap below, which already did this.
                    self.refreshKnobMode()
                    self.refreshStatus()
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.isBindingSlots = false
                    self?.statusMessage = "Bind error: \(error.localizedDescription)"
                }
            }
        }
    }

    func uploadKeymap(
        yaml: String,
        layer: Int? = nil,
        completion: @escaping @MainActor @Sendable (Result<String, Error>) -> Void
    ) {
        Task.detached(priority: .userInitiated) {
            do {
                let msg = try SocketClient.shared.uploadKeymap(yaml: yaml, layer: layer)
                await MainActor.run { [weak self] in
                    self?.statusMessage = msg
                    completion(.success(msg))
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.statusMessage = "Upload error: \(error.localizedDescription)"
                    completion(.failure(error))
                }
            }
        }
    }

    func readSlots(
        group: UInt8? = nil,
        counters: [UInt8]? = nil,
        completion: @escaping @MainActor @Sendable (Result<[[String: Any]], Error>) -> Void
    ) {
        Task.detached(priority: .userInitiated) {
            do {
                let res = try SocketClient.shared.readSlots(group: group, counters: counters)
                let slots = res["slots"] as? [[String: Any]] ?? []
                let box = SlotDumpBox(slots: slots)
                await MainActor.run {
                    completion(.success(box.slots))
                }
            } catch {
                await MainActor.run {
                    completion(.failure(error))
                }
            }
        }
    }

    func sendRawPacket(hexString: String, completion: @escaping @MainActor @Sendable (Result<String, Error>) -> Void) {
        let parts = hexString.components(separatedBy: CharacterSet.whitespacesAndNewlines.union(.punctuationCharacters))
            .filter { !$0.isEmpty }
        Task.detached(priority: .userInitiated) {
            do {
                let res = try SocketClient.shared.sendRaw(bytes: parts)
                let count = res["bytes_sent"] as? Int ?? parts.count
                await MainActor.run {
                    completion(.success("Sent \(count) bytes"))
                }
            } catch {
                await MainActor.run {
                    completion(.failure(error))
                }
            }
        }
    }

    func getHardwareLedMode(layer: Int, completion: @escaping @MainActor @Sendable (Int?) -> Void) {
        Task.detached(priority: .userInitiated) {
            let res = try? SocketClient.shared.getLed(layer: layer)
            let mode = res?["mode"] as? Int
            await MainActor.run {
                completion(mode)
            }
        }
    }
}
