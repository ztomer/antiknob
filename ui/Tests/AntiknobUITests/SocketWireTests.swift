// Tests for the JSON-RPC wire format the UI speaks to antiknob-daemon.
//
// These are the paths that were unreachable while framing and parsing lived
// inside a method that opened a socket first. Both failure modes here are
// silent in production: a mis-shaped request gets a refusal the UI reports as
// "offline", and an error reply read as a result makes the UI act as though a
// write succeeded.

import Foundation
import Testing

@testable import AntiknobUI

private func decodeLine(_ line: String) throws -> [String: Any] {
    let data = Data(line.utf8)
    let obj = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    return obj
}

@Suite("JSON-RPC wire format")
struct SocketWireTests {
    @Test("a request carries the fields the daemon dispatches on")
    func requestShape() throws {
        let line = try SocketWire.encodeRequest(id: 7, method: "get_status", params: [:])
        let obj = try decodeLine(line)

        #expect(obj["jsonrpc"] as? String == "2.0")
        #expect(obj["id"] as? Int == 7)
        #expect(obj["method"] as? String == "get_status")
        #expect(obj["params"] is [String: Any])
    }

    /// The daemon reads one request per line; without the terminator it waits
    /// for the rest of a line that never comes, and the UI reports a timeout
    /// as "daemon offline".
    @Test("a request is newline-terminated, exactly once")
    func requestIsOneLine() throws {
        let line = try SocketWire.encodeRequest(id: 1, method: "ping", params: [:])
        #expect(line.hasSuffix("\n"))
        #expect(line.filter { $0 == "\n" }.count == 1)
    }

    @Test("params are carried through")
    func paramsSurvive() throws {
        let line = try SocketWire.encodeRequest(
            id: 2, method: "set_led", params: ["layer": 1, "mode": "backlight"]
        )
        let params = try #require(try decodeLine(line)["params"] as? [String: Any])
        #expect(params["layer"] as? Int == 1)
        #expect(params["mode"] as? String == "backlight")
    }

    @Test("a value JSON cannot represent is refused, not silently dropped")
    func unencodableParamsThrow() {
        #expect(throws: SocketError.self) {
            try SocketWire.encodeRequest(id: 3, method: "x", params: ["bad": Data([0xFF])])
        }
    }

    @Test("a result is returned")
    func resultIsReturned() throws {
        let data = Data(#"{"jsonrpc":"2.0","id":1,"result":{"connected":true}}"#.utf8)
        let result = try #require(try SocketWire.decodeResponse(data) as? [String: Any])
        #expect(result["connected"] as? Bool == true)
    }

    /// The one that matters: a refusal must not read as success.
    @Test("an error reply throws instead of returning a result")
    func errorReplyThrows() {
        let data = Data(#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"busy"}}"#.utf8)
        #expect(throws: SocketError.self) {
            try SocketWire.decodeResponse(data)
        }
        do {
            _ = try SocketWire.decodeResponse(data)
            Issue.record("an error reply was accepted as a result")
        } catch let error as SocketError {
            #expect(error.errorDescription?.contains("busy") == true,
                    "the daemon's message was lost: \(error)")
        } catch {
            Issue.record("unexpected error type: \(error)")
        }
    }

    @Test("an error reply with no message still throws")
    func errorWithoutMessageThrows() {
        let data = Data(#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000}}"#.utf8)
        #expect(throws: SocketError.self) { try SocketWire.decodeResponse(data) }
    }

    @Test("malformed JSON is a parse error naming what arrived")
    func malformedReplyThrows() {
        do {
            _ = try SocketWire.decodeResponse(Data("not json at all".utf8))
            Issue.record("garbage was accepted")
        } catch let error as SocketError {
            #expect(error.errorDescription?.contains("not json at all") == true,
                    "the reply is not quoted back, so the failure is undiagnosable")
        } catch {
            Issue.record("unexpected error type: \(error)")
        }
    }

    /// A reply with neither result nor error is well-formed JSON; returning
    /// an empty result is the documented behaviour, not a crash.
    @Test("a reply with no result yields an empty result")
    func missingResultIsEmpty() throws {
        let data = Data(#"{"jsonrpc":"2.0","id":1}"#.utf8)
        let result = try SocketWire.decodeResponse(data)
        #expect((result as? [String: Any])?.isEmpty == true)
    }

    /// `bind_slots` speaks the registry contract: singular `layer`, omitted
    /// for all layers, and no `dry_run` (registry: CLI-only). The client
    /// used to send `dry_run` always and the daemon ignored it.
    @Test("bind_slots params use the singular layer and no dry_run")
    func bindSlotsParamsShape() {
        let all = SocketWire.bindSlotsParams(layer: nil)
        #expect(all["layer"] == nil)
        #expect(all["layers"] == nil)
        #expect(all["dry_run"] == nil)
        #expect(all["dryRun"] == nil)

        let one = SocketWire.bindSlotsParams(layer: 1)
        #expect(one["layer"] as? Int == 1)
        #expect(one["layers"] == nil)
        #expect(one["dry_run"] == nil)
    }

    /// The summary is built from the daemon's read-back, never a literal.
    /// The client used to read a `status` key the daemon never sends, so a
    /// `?? "OK"` fallback reported success with a string the app made up.
    @Test("bind_slots message names what the daemon flashed")
    func bindSlotsMessageFromReadback() throws {
        let msg = try SocketWire.bindSlotsMessage(from: [
            "ok": true, "flashed_slots": 15,
            "layers": [0, 1, 2], "key_ids": [2, 3, 4, 5, 6]
        ])
        #expect(msg.contains("15"))
        #expect(msg.contains("0, 1, 2"))
        #expect(msg.contains("2, 3, 4, 5, 6"))
        #expect(!msg.contains("OK"))
    }

    @Test("bind_slots message without a count throws instead of inventing one")
    func bindSlotsMessageWithoutCountThrows() {
        #expect(throws: SocketError.self) {
            try SocketWire.bindSlotsMessage(from: ["ok": true])
        }
        // The old fallback shape must never read as success again.
        #expect(throws: SocketError.self) {
            try SocketWire.bindSlotsMessage(from: ["status": "OK"])
        }
    }
}
