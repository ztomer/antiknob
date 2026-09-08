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

/// A view holds an index; the array behind it can shrink underneath.
///
/// SwiftUI re-evaluates a body with the index it captured, so deleting a
/// layer lets the removed view's body run once more against the shortened
/// array. `cfg.layers[idx]` TRAPS there. It crashed the app on the delete
/// confirmation -- `Swift runtime failure: Index out of range`, from
/// `LayerLighting.selection` -- with thirty-nine more direct subscripts
/// behind it waiting for the same moment.
@Suite("Stale layer indices")
@MainActor
struct StaleLayerIndexTests {
    /// `ConfigStore(inMemory:)`, never `ConfigStore()`.
    ///
    /// The plain initialiser loads the real config and writes back on every
    /// `cfg` assignment, so the first version of this helper edited the
    /// user's live configuration from a unit test -- it renamed a layer to
    /// "Browse" and pushed it to the running daemon.
    private func store(_ names: [String]) -> ConfigStore {
        ConfigStore(inMemory: Config(
            layers: names.map { LayerConfig(name: $0) },
            doubleTapSwitch: true,
            doubleTapWindow: 0.25,
            layerHotkey: nil,
            layerHotkeyBack: nil,
            scrollLinesPerDetent: 3
        ))
    }

    @Test("reading past the end returns nothing instead of trapping")
    func readingPastTheEndIsNil() {
        let s = store(["Media", "Navigate"])
        #expect(s[layer: 0]?.name == "Media")
        #expect(s[layer: 1]?.name == "Navigate")
        #expect(s[layer: 2] == nil)
        #expect(s[layer: 99] == nil)
        #expect(s[layer: -1] == nil)
    }

    /// Writing through a stale index does nothing, because the layer that
    /// write was meant for is gone. Appending instead, or wrapping to
    /// layer 0, would edit a layer the user is not looking at.
    @Test("writing past the end changes nothing")
    func writingPastTheEndIsANoOp() {
        let s = store(["Media"])
        s[layer: 4]?.name = "Ghost"
        s[layer: -1]?.name = "Ghost"
        #expect(s.cfg.layers.count == 1)
        #expect(s.cfg.layers[0].name == "Media")
    }

    @Test("a write through a live index still lands")
    func writingThroughALiveIndexWorks() {
        let s = store(["Media", "Navigate"])
        s[layer: 1]?.name = "Browse"
        s[layer: 1]?.led = "green"
        #expect(s.cfg.layers[1].name == "Browse")
        #expect(s.cfg.layers[1].led == "green")
    }

    /// The exact sequence that crashed: two layers, a view holding index 1,
    /// the layer at 1 deleted, the view's body re-read.
    @Test("the delete that crashed the app now reads as nothing")
    func theDeleteSequenceIsSurvivable() {
        let s = store(["Media", "Navigate"])
        let heldByAView = 1
        s.cfg.layers.remove(at: 1)
        #expect(s[layer: heldByAView] == nil)
        #expect(s[layer: heldByAView]?.led.flatMap(LedMode.named) == nil)
        s[layer: heldByAView]?.led = "red"
        #expect(s.cfg.layers.count == 1)
    }

    /// The name field's binding is created while the body runs, so it has to
    /// tolerate the same moment the read does.
    @Test("the name binding survives its layer being deleted")
    func theNameBindingSurvivesDeletion() {
        let s = store(["Media", "Navigate"])
        let binding = s.layerName(1)
        #expect(binding.wrappedValue == "Navigate")
        s.cfg.layers.remove(at: 1)
        #expect(binding.wrappedValue == "")
        binding.wrappedValue = "Ghost"
        #expect(s.cfg.layers.count == 1)
        #expect(s.cfg.layers[0].name == "Media")
    }
}

/// No test may write to the user's configuration.
///
/// `ConfigStore()` loads the real `host.json` and pushes every `cfg`
/// assignment back to the daemon, so a test that builds one and assigns a
/// fixture is not testing a store -- it is editing the machine it runs on.
/// One did: it renamed a layer to "Browse", set it green, and sent both to
/// the running daemon, destroying a restored config three times before the
/// file backups made the cause visible.
@Suite("Test stores are inert")
@MainActor
struct InMemoryStoreTests {
    private var fixture: Config {
        Config(
            layers: [LayerConfig(name: "Fixture")],
            doubleTapSwitch: true,
            doubleTapWindow: 0.25,
            layerHotkey: nil,
            layerHotkeyBack: nil,
            scrollLinesPerDetent: 3
        )
    }

    /// The path a persisting store takes on every edit. An in-memory store
    /// must not reach it, so calling it directly has to be a no-op rather
    /// than a write.
    /// Asserts on the ATTEMPT, not on the file.
    ///
    /// The first version of this test compared `host.json` before and after
    /// and passed with the guard deleted: the write is a detached Task, so
    /// the file had not changed yet when the test looked. A guard that
    /// cannot fail is not a guard, so this counts the attempt, which
    /// `applyConfig` records before it goes async.
    @Test("an in-memory store never sets out to persist")
    func inMemoryStoreDoesNotPersist() {
        let s = ConfigStore(inMemory: fixture)
        #expect(s.persistAttempts == 0)

        s.cfg.layers[0].name = "Edited"
        s.cfg.layers[0].led = "rainbow"
        s.applyConfig(s.cfg)

        #expect(s.cfg.layers[0].name == "Edited", "the store still holds its own edits")
        #expect(s.persistAttempts == 0, "a test store tried to write the real config")
    }

    /// It starts from the fixture, not from whatever is installed on the
    /// machine -- otherwise every assertion depends on the developer's own
    /// configuration.
    @Test("an in-memory store starts from its fixture, not from disk")
    func inMemoryStoreIgnoresDisk() {
        let s = ConfigStore(inMemory: fixture)
        #expect(s.cfg.layers.count == 1)
        #expect(s.cfg.layers[0].name == "Fixture")
    }
}
