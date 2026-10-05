// The app's ENCODER, checked against the daemon's decoder.
//
// The wire format is implemented twice — once in Rust for `HostAction`, once in
// Swift for `Action` — and a rename on one side is silent on the other: it does
// not fail to build, it does not throw at runtime, it just produces a file the
// other half cannot read. `WireFormatTests` above pins that each spelling
// DECODES. This file pins that each spelling is what gets WRITTEN, which is
// the half that was wrong.
//
// `tests/swift_rust_model_parity.rs` on the Rust side decodes the same
// expectations with the real `serde`, so a spelling cannot be "correct" here
// and unreadable there without one of the two failing.
//
// Measured drift these exist for, all fixed 2026-10-04:
//   * `openURL` written, `openUrl` read — the whole host.json became
//     undecodable, so the daemon's `load_json` substituted the defaults and
//     every layer was lost.
//   * `bundleId` written, `bundle_id` read — same, for launchApp and quitApp.
//   * `delayMs` written, `delay_ms` read — parsed without complaint, delay read
//     back nil, every inter-step pause silently dropped.
//   * no `variants` field at all — a save from this app deleted every virtual
//     layer the daemon had.

import Foundation
import Testing

@testable import AntiknobUI

private func encoded(_ value: some Encodable) throws -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    // Failable on purpose, and unwrapped here rather than defaulted: a JSON
    // encoding that is not valid UTF-8 would mean the encoder produced
    // something unprintable, and silently substituting replacement characters
    // would hide it behind assertions that pass.
    let data = try encoder.encode(value)
    guard let text = String(data: data, encoding: .utf8) else {
        throw EncodingError.invalidValue(
            value,
            EncodingError.Context(codingPath: [], debugDescription: "encoder produced non-UTF-8")
        )
    }
    return text
}

@Suite("the app writes what the daemon reads")
struct EncoderSpellingTests {
    /// The `type` tag is the variant name under the daemon's
    /// `#[serde(tag = "type", rename_all = "camelCase")]`. `openURL` reads as
    /// `openUrl` there, so writing `openURL` produced a file nothing could
    /// load — and because the failure is "unparseable", not "wrong", the
    /// daemon replaced the user's whole configuration with the defaults.
    @Test("an openURL action is written as openUrl")
    func openUrlIsWrittenInTheDaemonsSpelling() throws {
        let text = try encoded(Action.openURL(url: "https://example.invalid"))
        #expect(text.contains("\"type\":\"openUrl\""))
        #expect(!text.contains("openURL"))
    }

    /// `rename_all` on an enum renames its VARIANTS. Renaming a variant's
    /// FIELDS needs `rename_all_fields`, which `HostAction` does not have, so
    /// the field is `bundle_id` — and it has no `#[serde(default)]`, so
    /// `bundleId` made the entire config undecodable rather than merely
    /// dropping one field.
    @Test("bundle ids are written snake_case")
    func bundleIdIsWrittenSnakeCase() throws {
        let launch = try encoded(Action.launchApp(bundleId: "com.apple.Safari"))
        #expect(launch.contains("\"bundle_id\":\"com.apple.Safari\""))

        let quit = try encoded(Action.quitApp(bundleId: "com.apple.Safari", force: true))
        #expect(quit.contains("\"bundle_id\":\"com.apple.Safari\""))
    }

    /// `SeqStep` declares no `rename_all`, so its field really is `delay_ms`.
    /// The old spelling did not error — serde ignored the unknown key — it
    /// produced a step with no delay, which is worse than an error.
    @Test("a sequence delay is written as delay_ms")
    func delayIsWrittenSnakeCase() throws {
        let text = try encoded(SeqStep(key: 8, mods: ["cmd"], label: "C", delayMs: 40))
        #expect(text.contains("\"delay_ms\":40"))
        #expect(!text.contains("delayMs"))
    }
}

@Suite("nothing the daemon writes is dropped on the way back out")
struct RoundTripPreservationTests {
    /// The daemon owns virtual layers and this app has no control that edits
    /// them. That is exactly why the field has to exist: a `Codable` type that
    /// does not know a key does not preserve it, it discards it — and this
    /// store writes the whole file on every edit. Before the field existed,
    /// the first unrelated change made here deleted every variant the daemon
    /// had, and the layers went inert with nothing to search for.
    @Test("a layer's variants survive a decode and encode")
    func variantsSurvive() throws {
        let original = LayerConfig(
            name: "Navigate",
            twistR: .launchApp(bundleId: "com.apple.Safari"),
            variants: [
                LayerVariant(
                    name: "Safari",
                    apps: ["com.apple.Safari"],
                    twistL: .aux(key: .volumeUp)
                )
            ]
        )
        let text = try encoded(original)
        #expect(text.contains("\"variants\""))
        #expect(text.contains("com.apple.Safari"))

        let again = try JSONDecoder().decode(LayerConfig.self, from: Data(text.utf8))
        #expect(again.variants == original.variants)
        #expect(again.variants.first?.apps == ["com.apple.Safari"])
        #expect(again.variants.first?.twistL == .aux(key: .volumeUp))
        #expect(again.twistR == .launchApp(bundleId: "com.apple.Safari"))
    }

    /// The daemon marks `variants` `skip_serializing_if = "Vec::is_empty"`, so
    /// an empty array is a key the file did not have. Round-tripping has to be
    /// a fixed point for a fixed layer, or every save grows the file.
    @Test("a fixed layer does not gain an empty variants key")
    func aFixedLayerGainsNothing() throws {
        let text = try encoded(LayerConfig(name: "Plain"))
        #expect(!text.contains("variants"))
    }

    /// The same argument as `variants`, one level up: `boundDeviceLayers` is
    /// what tells the app which device layer the daemon can hear, and losing it
    /// makes a fully flashed knob report that no host layer can fire.
    ///
    /// Asserted as IDEMPOTENCE rather than `decode(encode(x)) == x`, because
    /// that equality is not a property this type has and never had:
    /// `LayerConfig.encode` writes `action ?? .none` for all five gestures —
    /// it must, since the daemon's `HostAction` is not an `Option` — so an
    /// unset gesture comes back as `.some(.none)` and compares unequal to the
    /// `nil` it started as. What a store that rewrites the file on every edit
    /// needs is the weaker, correct property: the SECOND write is byte-identical
    /// to the first, so no save accumulates drift.
    @Test("writing the config twice changes nothing the second time")
    func configEncodingIsIdempotent() throws {
        let original = Config(
            layers: [LayerConfig(name: "One", led: "rainbow")],
            doubleTapSwitch: false,
            doubleTapWindow: 0.4,
            layerHotkey: KeyChordSpec(key: 43, mods: ["cmd"], label: ","),
            layerHotkeyBack: nil,
            scrollLinesPerDetent: 7,
            boundDeviceLayers: [0, 1, 2]
        )
        let decoder = JSONDecoder()
        let first = try encoded(original)
        let second = try encoded(decoder.decode(Config.self, from: Data(first.utf8)))
        #expect(first == second)

        let back = try decoder.decode(Config.self, from: Data(first.utf8))
        #expect(back.boundDeviceLayers == [0, 1, 2])
        #expect(back.doubleTapSwitch == false)
        #expect(back.doubleTapWindow == 0.4)
        #expect(back.scrollLinesPerDetent == 7)
        #expect(back.layers.first?.led == "rainbow")
    }
}

/// An older build of this app wrote the wrong spellings, and those files are
/// somebody's configuration. Reading them has to keep working.
@Suite("files written by older builds still load")
struct LegacySpellingTests {
    @Test("the openURL spelling is still accepted")
    func openURLStillDecodes() throws {
        #expect(
            try JSONDecoder().decode(Action.self, from: Data(#"{"type":"openURL","url":"x"}"#.utf8))
                == .openURL(url: "x")
        )
    }

    @Test("the bundleId spelling is still accepted")
    func bundleIdStillDecodes() throws {
        let json = #"{"type":"launchApp","bundleId":"com.apple.Safari"}"#
        #expect(
            try JSONDecoder().decode(Action.self, from: Data(json.utf8))
                == .launchApp(bundleId: "com.apple.Safari")
        )
    }

    @Test("the delayMs spelling is still accepted")
    func delayMsStillDecodes() throws {
        let json = #"{"key":8,"mods":["cmd"],"label":"C","delayMs":120}"#
        #expect(try JSONDecoder().decode(SeqStep.self, from: Data(json.utf8)).delayMs == 120)
    }
}
