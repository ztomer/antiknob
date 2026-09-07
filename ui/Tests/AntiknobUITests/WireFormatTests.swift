// Tests for the host.json wire format.
//
// The daemon writes snake_case; the UI's own encoder writes camelCase; both
// must decode. Every one of these dual-key paths is a `try?` chain that
// silently yields nil on a mismatch -- a rename would not fail a build or
// throw at runtime, it would just quietly drop the user's binding. These pin
// the pairs and the round trip.

import Foundation
import Testing

@testable import AntiknobUI

private let decoder = JSONDecoder()
private let encoder = JSONEncoder()

private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
    try decoder.decode(type, from: Data(json.utf8))
}

@Suite("host.json wire format")
struct WireFormatTests {
    @Test("a layer decodes from the daemon's snake_case gesture keys")
    func layerDecodesSnakeCase() throws {
        let layer = try decode(LayerConfig.self, """
        {"name":"Navigate",
         "twist_l":{"type":"scroll","lines":3},
         "twist_r":{"type":"scroll","lines":-3},
         "hold_twist_l":{"type":"aux","key":"volumeUp"},
         "hold_twist_r":{"type":"mouseClick","button":"left"},
         "press":{"type":"keyChord","key":49,"mods":["cmd"],"label":"Space"}}
        """)

        #expect(layer.name == "Navigate")
        #expect(layer.twistL == .scroll(lines: 3))
        #expect(layer.twistR == .scroll(lines: -3))
        #expect(layer.holdTwistR == .mouseClick(button: .left))
        #expect(layer.press == .keyChord(key: 49, mods: ["cmd"], label: "Space"))
    }

    @Test("the same layer decodes from camelCase keys")
    func layerDecodesCamelCase() throws {
        let layer = try decode(LayerConfig.self, """
        {"name":"Navigate",
         "twistL":{"type":"scroll","lines":3},
         "holdTwistL":{"type":"scroll","lines":1}}
        """)
        #expect(layer.twistL == .scroll(lines: 3))
        #expect(layer.holdTwistL == .scroll(lines: 1))
    }

    /// The encoder writes camelCase, so a round trip only survives if the
    /// decoder accepts that half of every pair.
    @Test("a layer survives an encode/decode round trip unchanged")
    func layerRoundTrips() throws {
        let original = LayerConfig(
            name: "Media",
            twistL: .scroll(lines: 5),
            twistR: .aux(key: .volumeUp),
            holdTwistL: .launchApp(bundleId: "com.apple.Safari"),
            holdTwistR: .openURL(url: "https://example.invalid"),
            press: .sequence(steps: [SeqStep(key: 8, mods: ["cmd"], label: "C", delayMs: 40)])
        )
        let again = try decoder.decode(LayerConfig.self, from: encoder.encode(original))
        #expect(again == original)
    }

    @Test("config decodes from the daemon's snake_case top-level keys")
    func configDecodesSnakeCase() throws {
        let cfg = try decode(Config.self, """
        {"layers":[{"name":"One"}],
         "double_tap_switch":false,
         "double_tap_window":0.4,
         "scroll_lines_per_detent":7}
        """)
        #expect(cfg.doubleTapSwitch == false)
        #expect(cfg.doubleTapWindow == 0.4)
        #expect(cfg.scrollLinesPerDetent == 7)
        #expect(cfg.layers.count == 1)
    }

    /// A config the daemon has never written must not come back with
    /// double-tap silently off -- the documented default is on.
    @Test("missing keys fall back to the documented defaults")
    func missingKeysUseDefaults() throws {
        let cfg = try decode(Config.self, #"{"layers":[]}"#)
        #expect(cfg.doubleTapSwitch == true)
        #expect(cfg.doubleTapWindow == 0.25)
        #expect(cfg.scrollLinesPerDetent == 3)
    }

    @Test("bundle ids decode from either spelling")
    func bundleIdAcceptsBothSpellings() throws {
        #expect(try decode(Action.self, #"{"type":"launchApp","bundle_id":"com.apple.Finder"}"#)
                == .launchApp(bundleId: "com.apple.Finder"))
        #expect(try decode(Action.self, #"{"type":"launchApp","bundleId":"com.apple.Finder"}"#)
                == .launchApp(bundleId: "com.apple.Finder"))
    }

    @Test("a sequence step's delay decodes from either spelling")
    func delayAcceptsBothSpellings() throws {
        #expect(try decode(SeqStep.self, #"{"key":8,"delay_ms":120}"#).delayMs == 120)
        #expect(try decode(SeqStep.self, #"{"key":8,"delayMs":120}"#).delayMs == 120)
    }

    /// An action type this build does not know must degrade to `none`, not
    /// throw -- a newer daemon writing a newer action would otherwise make
    /// the whole config undecodable and drop every layer.
    @Test("an unknown action type degrades to none instead of throwing")
    func unknownActionDegrades() throws {
        #expect(try decode(Action.self, #"{"type":"teleport","destination":"mars"}"#) == Action.none)
    }

    @Test("a chord renders its modifiers in the macOS glyph order")
    func chordDisplayUsesGlyphOrder() {
        let chord = KeyChordSpec(key: 0, mods: ["cmd", "shift", "ctrl", "opt"], label: "A")
        #expect(chord.display == "⌃⌥⇧⌘A")
        #expect(KeyChordSpec(key: 0, mods: ["alt"], label: "B").display == "⌥B")
    }
}
