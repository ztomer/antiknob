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
    func setLed(layer: Int, mode: String, color: String?) {
        Task.detached(priority: .userInitiated) {
            do {
                let status = try SocketClient.shared.setLed(layer: layer, mode: mode, color: color)
                await MainActor.run { [weak self] in
                    self?.statusMessage = status
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.statusMessage = "LED error: \(error.localizedDescription)"
                }
            }
        }
    }

    public func bindSlots(layer: Int? = nil) {
        isBindingSlots = true
        Task.detached(priority: .userInitiated) {
            do {
                let status = try SocketClient.shared.bindSlots(layer: layer, dryRun: false)
                await MainActor.run { [weak self] in
                    self?.isBindingSlots = false
                    self?.statusMessage = status
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
