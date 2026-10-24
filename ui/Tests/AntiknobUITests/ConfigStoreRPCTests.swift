// ConfigStore's one-shot daemon commands, success AND failure.
//
// Every one of these used to reach `SocketClient.shared` directly, so the
// only way to exercise them was a live daemon and the message the user reads
// after each (`statusMessage`, the completion result) was checked by
// nobody. They run through `DaemonRPC` now, with a hand-written fake.
//
// Each failure test asserts the text, not just that something failed: the
// statusMessage is the only place the user learns a write did not happen.

import Foundation
import Testing

@testable import AntiknobUI

/// Waits for a main-actor condition the store reaches from a detached task.
/// Bounded, so a store that never gets there fails instead of hanging.
@MainActor
private func eventually(_ condition: @MainActor () -> Bool) async -> Bool {
    let deadline = ContinuousClock.now + .seconds(5)
    while ContinuousClock.now < deadline {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(5))
    }
    return condition()
}

/// A completion's result carried back to the test. Not `Sendable` in general
/// (a slot dump is `[[String: Any]]`); safe here because the completion runs
/// once and the test reads it only after it has.
private struct Delivered<T>: @unchecked Sendable {
    let result: Result<T, Error>
}

private struct Refused: LocalizedError {
    var errorDescription: String? { "refused by the fake" }
}

/// What both suites share: a store over a fake, and a way to wait for a
/// completion-style command.
@MainActor
private protocol RPCSuite {}

extension RPCSuite {
    func config(led: String? = nil) -> Config {
        var layer = LayerConfig(name: "Media")
        layer.led = led
        return Config(
            layers: [layer, LayerConfig(name: "Navigate")],
            doubleTapSwitch: true,
            doubleTapWindow: 0.25,
            layerHotkey: nil,
            layerHotkeyBack: nil,
            scrollLinesPerDetent: 3
        )
    }

    func store(_ rpc: FakeDaemonRPC, led: String? = nil) -> ConfigStore {
        ConfigStore(inMemory: config(led: led), rpc: rpc)
    }

    /// Run a completion-style command and wait for its completion.
    func outcome<T>(
        _ start: (@escaping @MainActor @Sendable (Result<T, Error>) -> Void) -> Void
    ) async -> Result<T, Error> {
        let delivered: Delivered<T> = await withCheckedContinuation { continuation in
            start { continuation.resume(returning: Delivered(result: $0)) }
        }
        return delivered.result
    }
}

@Suite("ConfigStore daemon writes")
@MainActor
struct ConfigStoreWriteRPCTests: RPCSuite {
    // MARK: - The seam itself

    @Test("a store built for tests reaches no daemon unless it is given one")
    func theDefaultSeamIsOffline() async {
        let s = ConfigStore(inMemory: config())
        let result: Result<String, Error> = await outcome { done in
            s.uploadKeymap(yaml: "x", completion: done)
        }
        guard case .failure(let error) = result else {
            Issue.record("an offline store reported success")
            return
        }
        #expect(error.localizedDescription.contains("offline"))
        #expect(s.statusMessage?.hasPrefix("Upload error: ") == true)
    }

    // MARK: - set_led

    @Test("setLed reports the mode the device kept, by name")
    func setLedSuccess() async {
        let rpc = FakeDaemonRPC()
        rpc.appliedLedMode = 3
        let s = store(rpc)
        let result: Result<AppliedLedMode, Error> = await outcome { done in
            s.setLed(layer: 1, mode: "breathing", completion: done)
        }
        guard case .success(let applied) = result else {
            Issue.record("expected success")
            return
        }
        #expect(applied.mode == 3)
        #expect(s.statusMessage == "Layer 2 backlight: \(applied.name)")
        #expect(rpc.calls == ["setLed(1, breathing)"])
    }

    @Test("setLed failure is reported, with the reason, and not as a mode")
    func setLedFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        let result: Result<AppliedLedMode, Error> = await outcome { done in
            s.setLed(layer: 0, mode: "off", completion: done)
        }
        guard case .failure = result else {
            Issue.record("expected failure")
            return
        }
        #expect(s.statusMessage == "LED error: refused by the fake")
    }

    // MARK: - set_layer

    @Test("switching layers moves the checkmark and asks the daemon for status")
    func setActiveLayerSuccess() async {
        let rpc = FakeDaemonRPC()
        let s = store(rpc)
        s.setActiveLayer(1)
        #expect(s.activeLayerIdx == 1, "moved at once, so the menu answers the click")
        #expect(await eventually { rpc.calls.contains("setLayer(1)") })
        // The reply triggers a status refresh, which asks the daemon.
        #expect(await eventually { rpc.calls.contains("isConnected()") })
        #expect(s.activeLayerIdx == 1)
    }

    @Test("a refused layer switch puts the checkmark back and says so")
    func setActiveLayerFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        s.activeLayerIdx = 0
        s.setActiveLayer(1)
        #expect(await eventually { s.statusMessage != nil })
        #expect(s.activeLayerIdx == 0)
        #expect(s.statusMessage == "Layer switch failed: refused by the fake")
    }

    // MARK: - bind_slots

    @Test("a flash from a layer that names a backlight mode reports exactly the daemon's text")
    func bindSlotsWithBacklight() async {
        let rpc = FakeDaemonRPC()
        let s = store(rpc, led: "breathing")
        s.bindSlots(layer: 2)
        #expect(s.isBindingSlots, "the button reads busy until the reply")
        #expect(await eventually { !s.isBindingSlots })
        #expect(s.statusMessage == "Flashed 15 slot bindings")
        #expect(rpc.calls.first == "bindSlots(2)")
        // The knob's mode changed underneath the panes: both are re-read.
        #expect(await eventually { rpc.calls.contains("getKnobMode()") })
        #expect(await eventually { rpc.calls.contains("isConnected()") })
    }

    @Test("a successful flash says when the light cannot have followed")
    func bindSlotsWithoutBacklight() async {
        let rpc = FakeDaemonRPC()
        let s = store(rpc, led: "  ")
        s.bindSlots()
        #expect(await eventually { s.statusMessage != nil })
        #expect(
            s.statusMessage
                == "Flashed 15 slot bindings — active layer names no backlight mode, light unchanged"
        )
        #expect(rpc.calls.first == "bindSlots(all)")
    }

    @Test("a failed flash clears the busy state and says why")
    func bindSlotsFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        s.bindSlots()
        #expect(await eventually { s.statusMessage != nil })
        #expect(!s.isBindingSlots)
        #expect(s.statusMessage == "Bind error: refused by the fake")
    }

    // MARK: - upload_keymap

    @Test("an upload reports the daemon's own message")
    func uploadSuccess() async {
        let rpc = FakeDaemonRPC()
        rpc.uploadMessage = "Flashed 3 layers"
        let s = store(rpc)
        let result: Result<String, Error> = await outcome { done in
            s.uploadKeymap(yaml: "layers: []", layer: 1, completion: done)
        }
        guard case .success(let message) = result else {
            Issue.record("expected success")
            return
        }
        #expect(message == "Flashed 3 layers")
        #expect(s.statusMessage == "Flashed 3 layers")
        #expect(rpc.calls == ["uploadKeymap(layers: [], 1)"])
    }

    @Test("a failed upload is an error message, not a success string")
    func uploadFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        let result: Result<String, Error> = await outcome { done in
            s.uploadKeymap(yaml: "x", completion: done)
        }
        guard case .failure = result else {
            Issue.record("expected failure")
            return
        }
        #expect(s.statusMessage == "Upload error: refused by the fake")
    }
}

@Suite("ConfigStore daemon reads")
@MainActor
struct ConfigStoreReadRPCTests: RPCSuite {
    // MARK: - read_slots

    @Test("read_slots hands back the slot list, and an empty one when the key is absent")
    func readSlotsSuccess() async {
        let rpc = FakeDaemonRPC()
        rpc.slotsReply = ["slots": [["key": 2, "layer": 1], ["key": 3, "layer": 1]]]
        let s = store(rpc)
        let result: Result<[[String: Any]], Error> = await outcome { done in
            s.readSlots(group: 15, counters: [1, 2], completion: done)
        }
        guard case .success(let slots) = result else {
            Issue.record("expected success")
            return
        }
        #expect(slots.count == 2)
        #expect(slots.first?["key"] as? Int == 2)
        #expect(rpc.calls == ["readSlots(15, [1, 2])"])

        rpc.slotsReply = [:]
        let none: Result<[[String: Any]], Error> = await outcome { done in
            s.readSlots(completion: done)
        }
        guard case .success(let empty) = none else {
            Issue.record("expected success")
            return
        }
        #expect(empty.isEmpty)
    }

    @Test("read_slots failure reaches the caller")
    func readSlotsFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        let result: Result<[[String: Any]], Error> = await outcome { done in
            s.readSlots(completion: done)
        }
        guard case .failure(let error) = result else {
            Issue.record("expected failure")
            return
        }
        #expect(error.localizedDescription == "refused by the fake")
    }

    // MARK: - send_raw

    @Test("a raw packet is split on whitespace and punctuation, and the count is the daemon's")
    func sendRawSuccess() async {
        let rpc = FakeDaemonRPC()
        rpc.rawReply = ["bytes_sent": 64]
        let s = store(rpc)
        let result: Result<String, Error> = await outcome { done in
            s.sendRawPacket(hexString: "03 fa,0f\n00", completion: done)
        }
        guard case .success(let message) = result else {
            Issue.record("expected success")
            return
        }
        #expect(message == "Sent 64 bytes")
        #expect(rpc.calls == ["sendRaw(03 fa 0f 00)"])
    }

    @Test("with no count in the reply the count is what was sent")
    func sendRawWithoutACount() async {
        let rpc = FakeDaemonRPC()
        let s = store(rpc)
        let result: Result<String, Error> = await outcome { done in
            s.sendRawPacket(hexString: "01 02 03", completion: done)
        }
        guard case .success(let message) = result else {
            Issue.record("expected success")
            return
        }
        #expect(message == "Sent 3 bytes")
    }

    @Test("a refused raw write is a failure")
    func sendRawFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        let result: Result<String, Error> = await outcome { done in
            s.sendRawPacket(hexString: "01", completion: done)
        }
        guard case .failure = result else {
            Issue.record("expected failure")
            return
        }
    }

    // MARK: - get_led

    @Test("the hardware LED mode is the number the daemon reports, else nil")
    func getLedModes() async {
        let rpc = FakeDaemonRPC()
        let s = store(rpc)

        rpc.ledReply = ["mode": 4]
        let known: Int? = await withCheckedContinuation { c in
            s.getHardwareLedMode(layer: 2) { c.resume(returning: $0) }
        }
        #expect(known == 4)
        #expect(rpc.calls == ["getLed(2)"])

        rpc.ledReply = ["mode": "not a number"]
        let wrongType: Int? = await withCheckedContinuation { c in
            s.getHardwareLedMode(layer: 0) { c.resume(returning: $0) }
        }
        #expect(wrongType == nil)

        rpc.failure = Refused()
        let failed: Int? = await withCheckedContinuation { c in
            s.getHardwareLedMode(layer: 0) { c.resume(returning: $0) }
        }
        #expect(failed == nil)
    }

    // MARK: - get_knob_mode

    @Test("the knob mode and the slots it sits on are recorded from the reply")
    func knobModeSuccess() async {
        let rpc = FakeDaemonRPC()
        rpc.knobModeReply = [
            "mode": "host-translate",
            "device_binding_summary": "layer 1 carries the chords",
            "buttons": 1,
            "knob_key_ids": [2, 3, 4, 5, 6]
        ]
        let s = store(rpc)
        s.refreshKnobMode()
        #expect(await eventually { s.knobModeRaw != nil })
        #expect(s.knobModeRaw == "host-translate")
        #expect(s.deviceBindingSummary == "layer 1 carries the chords")
        #expect(s.knobButtons == 1)
        #expect(s.knobKeyIds == [2, 3, 4, 5, 6])
    }

    @Test("a failed read leaves the mode unknown, never one of the two real ones")
    func knobModeFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.knobModeReply = ["mode": "standalone"]
        let s = store(rpc)
        s.refreshKnobMode()
        #expect(await eventually { s.knobModeRaw == "standalone" })

        rpc.failure = Refused()
        s.refreshKnobMode()
        #expect(await eventually { s.knobModeRaw == nil })
        #expect(s.deviceBindingSummary == nil)
        #expect(s.knobButtons == nil)
        #expect(s.knobKeyIds == nil)
    }

    // MARK: - list_commands

    @Test("the command table comes from the daemon")
    func commandsSuccess() async throws {
        let rpc = FakeDaemonRPC()
        let command = try #require(
            DaemonCommand(json: ["name": "set_layer", "about": "Switch layer."])
        )
        rpc.commands = [command]
        let s = store(rpc)
        let got = try await s.fetchCommands()
        #expect(got.map(\.name) == ["set_layer"])
    }

    @Test("a daemon that cannot list its commands throws, so the pane can say so")
    func commandsFailure() async {
        let rpc = FakeDaemonRPC()
        rpc.failure = Refused()
        let s = store(rpc)
        await #expect(throws: (any Error).self) {
            _ = try await s.fetchCommands()
        }
    }

    // MARK: - status

    @Test("a connected daemon's status is what the banner shows, and the socket in use is named")
    func statusFromTheDaemon() async {
        let rpc = FakeDaemonRPC()
        rpc.path = "/tmp/antiknob.sock"
        rpc.status = [
            "devices": [["product": "Anticater VK01"]],
            "transport": "usb",
            "tap_active": true,
            "active_layer": 1
        ]
        let s = store(rpc)
        s.refreshStatus()
        #expect(await eventually { s.daemonConnected })
        #expect(await eventually { s.socketPath != nil })
        #expect(s.hardwareConnected)
        #expect(s.transport == "usb")
        #expect(s.tapActive)
        #expect(s.activeLayerIdx == 1)
        #expect(s.socketPath == "/tmp/antiknob.sock")
    }
}
