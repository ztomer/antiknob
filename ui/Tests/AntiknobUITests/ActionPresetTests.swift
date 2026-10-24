// The action picker's mapping, now that it is a pure function.
//
// It lived in two parallel switches inside a SwiftUI extension: one naming
// the dropdown's current choice, one mapping to the picker's selection. They
// had to agree case for case and could not be tested, because a test would
// have needed a view. A disagreement shows as a dropdown that says one thing
// while the picker beneath it selects another.

import Foundation
import Testing

@testable import AntiknobUI

@Suite("Action picker mapping")
struct ActionPresetTests {
    private let chord = Action.keyChord(key: 8, mods: ["cmd"], label: "C")

    @Test("each action lands on the picker entry that names it")
    func everyActionHasItsEntry() {
        let cases: [(Action, ActionPreset)] = [
            (.none, .none),
            (chord, .keystroke),
            (.sequence(steps: []), .sequence),
            (.launchApp(bundleId: "com.apple.Safari"), .openApp),
            (.openURL(url: "https://example.com"), .openURL),
            (.openPath(path: "/tmp"), .openPath),
            (.quitApp(bundleId: "com.apple.Safari", force: nil), .quitApp),
            (.hotkeySwitch(first: nil, second: nil), .hotkeySwitch),
            (.aux(key: .mute), .aux(.mute))
        ]
        for (action, preset) in cases {
            #expect(action.preset(defaultScrollLines: 3) == preset, "\(action)")
        }
    }

    /// The picker has no entry for a click, so it selects "None"; the
    /// dropdown's title says which click (see `LayerDetail.currentTitle`).
    @Test("a mouse click selects no picker entry")
    func mouseClickIsNone() {
        #expect(Action.mouseClick(button: .left).preset(defaultScrollLines: 3) == .none)
    }

    @Test("scroll direction follows its own count, else the default")
    func scrollDirection() {
        #expect(Action.scroll(lines: 5).preset(defaultScrollLines: -3) == .scrollUp)
        #expect(Action.scroll(lines: -5).preset(defaultScrollLines: 3) == .scrollDown)
        #expect(Action.scroll(lines: nil).preset(defaultScrollLines: 3) == .scrollUp)
        #expect(Action.scroll(lines: nil).preset(defaultScrollLines: -3) == .scrollDown)
        // Zero is "up": the comparison is >= 0, as the dropdown always read it.
        #expect(Action.scroll(lines: 0).preset(defaultScrollLines: -3) == .scrollUp)
    }

    @Test("the two brightness-external keys select their unified entry")
    func externalBrightnessIsUnified() {
        #expect(
            Action.aux(key: .brightnessUpExternal).preset(defaultScrollLines: 3)
                == .aux(.brightnessUp)
        )
    }

    /// A preset with no title would read "None" while selecting something
    /// else. Walks every case, so a new one added without a title is red.
    @Test("every preset has a title of its own")
    func everyPresetHasATitle() {
        let fixed: [ActionPreset] = [
            .none, .scrollUp, .scrollDown, .keystroke, .openApp,
            .sequence, .openURL, .openPath, .quitApp, .hotkeySwitch
        ]
        #expect(Set(fixed.map(\.title)).count == fixed.count, "titles must be distinct")
        for preset in fixed {
            #expect(ActionPreset.fixedTitles[preset] != nil, "\(preset) has no title")
        }
        #expect(ActionPreset.fixedTitles.count == fixed.count)
        for key in AuxKey.allCases {
            #expect(!ActionPreset.aux(key).title.isEmpty)
        }
        #expect(ActionPreset.aux(.mute).title == "Mute")
    }

    @Test("a media key with no listed title still reads as media")
    func unlistedAuxKeyFallsBack() {
        let unlisted = AuxKey.allCases.filter { auxTitles[$0] == nil }
        for key in unlisted {
            #expect(ActionPreset.aux(key).title == "Media")
        }
    }

    /// The Codable split must not have moved a byte of the wire format. One
    /// round trip per family; the spelling itself is pinned by
    /// `EncoderSpellingTests`.
    @Test("both families of action survive a round trip")
    func codingRoundTrips() throws {
        let actions: [Action] = [
            .none, .scroll(lines: -2), chord, .sequence(steps: []), .aux(key: .playPause),
            .mouseClick(button: .right), .launchApp(bundleId: "a.b"), .openURL(url: "https://x"),
            .openPath(path: "/p"), .quitApp(bundleId: "a.b", force: true),
            .hotkeySwitch(first: nil, second: nil)
        ]
        for action in actions {
            let data = try JSONEncoder().encode(action)
            #expect(try JSONDecoder().decode(Action.self, from: data) == action, "\(action)")
        }
        let unknown = try JSONDecoder().decode(Action.self, from: Data(#"{"type":"teleport"}"#.utf8))
        #expect(unknown == .none, "an unknown type reads as none, as it always did")
        let camel = Data(#"{"type":"launchApp","bundleId":"x.y"}"#.utf8)
        #expect(try JSONDecoder().decode(Action.self, from: camel) == .launchApp(bundleId: "x.y"))
    }
}
