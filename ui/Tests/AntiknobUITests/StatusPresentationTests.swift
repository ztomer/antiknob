// Tests for the status bar's presentation mapping.
//
// The bar renders transport and power as glyphs with the words only in the
// tooltip, so a wrong mapping is invisible until someone hovers. These pin
// each transport to a distinct glyph and pin power to the daemon's own text.

import Testing

@testable import AntiknobUI

private func status(
    _ transport: String,
    power: String = "Wired (USB Bus Powered)",
    connected: Bool = true
) -> StatusPresentation {
    StatusPresentation(transport: transport, powerDescription: power, connected: connected)
}

@Suite("Status presentation")
struct StatusPresentationTests {
    /// Driven by `Transport.allCases`, so adding a transport extends this test
    /// automatically. A new case that reuses another's label or glyph fails
    /// here; one that is never rendered fails to compile in `transportColor`.
    @Test("every transport maps to its own label and glyph")
    func transportsAreDistinct() {
        let all = Transport.allCases
        #expect(all.count >= 3, "transports disappeared: \(all)")

        let labels = all.map(\.displayName)
        let icons = all.map(\.icon)
        #expect(Set(labels).count == all.count, "two transports share a label: \(labels)")
        #expect(Set(icons).count == all.count, "two transports share a glyph: \(icons)")
        #expect(!labels.contains(""), "a transport has no label")
        #expect(!icons.contains(""), "a transport has no glyph")

        #expect(status("usb").transportDisplay == "USB (Wired)")
        #expect(status("wireless_2_4g").transportDisplay == "2.4GHz Wireless")
        #expect(status("bluetooth").transportDisplay == "Bluetooth Wireless")
    }

    /// The raw values are the daemon's wire strings; renaming one silently
    /// stops the UI recognising that link.
    @Test("transport raw values match the daemon's wire strings")
    func rawValuesMatchTheWire() {
        #expect(Transport(rawValue: "usb") == .usb)
        #expect(Transport(rawValue: "wireless_2_4g") == .wireless24GHz)
        #expect(Transport(rawValue: "bluetooth") == .bluetooth)
        #expect(Transport(rawValue: "disconnected") == nil)
    }

    @Test("an unrecognised transport falls back rather than showing a raw tag")
    func unknownTransportFallsBack() {
        let odd = status("thunderbolt-over-carrier-pigeon")
        #expect(odd.transportDisplay == "Disconnected")
        #expect(odd.transportIcon == "circle.slash")
    }

    @Test("power glyph follows the daemon's own description")
    func powerFollowsDescription() {
        #expect(status("usb", power: "Wired (USB Bus Powered)").powerIcon == "powerplug.fill")
        #expect(status("bluetooth", power: "Bluetooth Wireless (Battery Powered)").powerIcon
                == "battery.100")
        #expect(status("wireless_2_4g", power: "2.4GHz Wireless (Battery Powered)").powerIcon
                == "battery.100")
    }

    /// "USB Bus Powered" contains "Powered" but not "battery"; matching on the
    /// wrong substring would show a battery for a bus-powered device.
    @Test("bus-powered is not mistaken for battery-powered")
    func busPowerIsNotBattery() {
        #expect(status("usb", power: "Wired (USB Bus Powered)").powerIcon != "battery.100")
    }

    @Test("disconnected reports no power source regardless of stale text")
    func disconnectedOverridesStaleText() {
        let stale = status("usb", power: "Bluetooth Wireless (Battery Powered)", connected: false)
        #expect(stale.powerIcon == "powerplug.slash")
    }
}

/// The mode NUMBER the firmware reports and the name shown for it. This was
/// a `ledModeNames` dictionary of its own, separate from the mode list the
/// picker rendered -- two tables answering the same question, which is how
/// the names drifted to the wrong device family for months while a test held
/// them there. One table now, so these read it and the picker reads it.
@Suite("LED mode names")
struct LedModeNameTests {
    @Test("every mode the firmware reports has its own name")
    func modesAreNamedAndDistinct() {
        let names = (0...5).map { LedMode.numbered($0)?.name }
        #expect(names.allSatisfy { $0 != nil }, "a firmware mode has no name: \(names)")
        #expect(Set(names.compactMap { $0 }).count == 6, "two modes share a name: \(names)")
    }

    /// This test used to pin "Press (Reactive)" for mode 4 and "Custom" for
    /// mode 5 -- the 1189:884x family's names, on a 514c:8850. So the table
    /// drifted from the Rust side for months while a test held it there: the
    /// assertion was not guarding the mapping, it was enforcing the defect.
    ///
    /// The names now say what the knob shows. Modes 1 and 2 are fixed
    /// colours, and mode 5 is a second multicoloured effect rather than
    /// something unsupported.
    @Test("the mode names are this device's, not the 884x family's")
    func modeNamesAreThisDevices() {
        #expect(LedMode.numbered(1)?.name == "Red")
        #expect(LedMode.numbered(2)?.name == "Green")
        #expect(LedMode.numbered(4)?.name == "Rainbow")
        #expect(LedMode.numbered(5)?.name == "RGB")
        let names = LedMode.all.map(\.name)
        for stale in ["Backlight", "Shock (Breathe)", "Shock 2 (Rapid)", "Press (Reactive)"] {
            #expect(!names.contains(stale), "884x name survives: \(stale)")
        }
    }

    /// A number this build has no name for must read as unrecognised rather
    /// than borrow a neighbour's label -- a firmware carrying a mode we
    /// cannot name is exactly the case a fabricated one would hide.
    @Test("a mode number the firmware should never send is reported as unrecognised")
    func unknownModeIsUnmapped() {
        #expect(LedMode.numbered(9) == nil)
        #expect(LedMode.describe(number: 9).contains("unrecognised"))
        #expect(LedMode.describe(number: 9).contains("9"))
    }

    /// The wire name and the mode number are one fact. `set_led` takes the
    /// name and `get_led` answers with the number, so the pane can only tell
    /// "the write took" from "the device kept its old mode" if the pairing
    /// here matches `LED_MODE_NAMES` in src/led.rs.
    @Test("each wire name sits at the mode number the firmware reports for it")
    func wireNamesMatchModeNumbers() {
        let expected = ["off", "red", "green", "ripple", "rainbow", "rgb"]
        for (number, id) in expected.enumerated() {
            #expect(LedMode.named(id)?.number == number, "\(id) is not mode \(number)")
        }
        #expect(LedMode.all.count == expected.count)
    }

    @Test("the applied mode reports the firmware's own answer")
    func appliedModeNamesItself() {
        #expect(AppliedLedMode(mode: 4).name == "Rainbow")
        #expect(AppliedLedMode(mode: 9).name.contains("unrecognised"))
    }
}

/// The layer pane must never present a host layer as live when the firmware
/// cannot produce it. These pin the three cases, including the one that says
/// "not sure" rather than guessing.
@Suite("Knob mode presentation")
struct ModePresentationTests {
    @Test("only host-translate can fire host layers")
    func onlyHostTranslateFires() {
        #expect(ModePresentation(rawMode: "host-translate").hostLayersCanFire)
        #expect(!ModePresentation(rawMode: "standalone").hostLayersCanFire)
        #expect(!ModePresentation(rawMode: "unknown").hostLayersCanFire)
    }

    /// A missing or unrecognised reading is its own state. Falling back to
    /// either real mode would be the app inventing a fact about hardware it
    /// has not read.
    @Test("an unread or unrecognised mode is neither of the others")
    func unknownIsItsOwnState() {
        for raw in [nil, "", "wat", "HOST-TRANSLATE"] {
            let p = ModePresentation(rawMode: raw)
            #expect(p.mode == .unknown, "raw: \(raw ?? "nil")")
            #expect(!p.hostLayersCanFire)
        }
    }

    /// Each state reads differently, and none of them reads as a fault.
    ///
    /// This replaces a suite that asserted a `banner` containing the word
    /// "inactive" beside a "Flash Slot Bindings" call to action -- an orange
    /// warning triangle for a knob that had simply not been set up yet,
    /// which made a working app look broken on first run. The three states
    /// still have to be distinguishable; they no longer have to alarm.
    @Test("every state has its own line, and none of them is a warning")
    func statusLinesAreDistinctAndCalm() {
        let lines = ["host-translate", "standalone", "unknown"]
            .map { ModePresentation(rawMode: $0).statusText }
        #expect(Set(lines).count == 3, "two states share a line: \(lines)")
        for line in lines {
            #expect(!line.isEmpty)
            for alarming in ["inactive", "cannot", "never", "fail", "error", "!"] {
                #expect(
                    !line.lowercased().contains(alarming),
                    "status reads as a fault (\(alarming)): \(line)"
                )
            }
        }
    }

    /// A flashed knob is offered a reflash, not the first-run wording.
    @Test("the action reads for the state it is in")
    func theActionMatchesTheState() {
        #expect(ModePresentation(rawMode: "host-translate").flashActionTitle == "Reflash")
        #expect(ModePresentation(rawMode: "standalone").flashActionTitle == "Flash Knob")
        #expect(ModePresentation(rawMode: "unknown").flashActionTitle == "Flash Knob")
    }
}

/// The LED controls must describe this device's own modes, and must not
/// offer a promise the firmware does not keep.
@Suite("LED mode list")
struct LedModeTests {
    @Test("the modes are the 8850's, not the 884x names")
    func modesMatchTheDevice() {
        let ids = LedMode.all.map(\.id)
        #expect(ids == ["off", "red", "green", "ripple", "rainbow", "rgb"], "\(ids)")
        // The old lists named effects this device does not have, and then
        // named effects for modes that are fixed colours.
        for stale in ["backlight", "shock", "press", "static", "reactive"] {
            #expect(!ids.contains(stale), "stale mode id survives: \(stale)")
        }
    }

    /// Mode 5 is offered. It was hidden as "crashes the firmware", which it
    /// does not -- the owner watched it render, and the vendor app sends it
    /// while walking its own mode buttons. Every mode the device has should
    /// be reachable, or the picker quietly withholds one.
    @Test("every mode the device has is offered, mode 5 included")
    func everyModeIsOffered() {
        let ids = Set(LedMode.all.map(\.id))
        #expect(ids.contains("rgb"), "mode 5 is not selectable: \(ids)")
        #expect(ids.count == 6)
    }

    @Test("every mode has a description and its own glyph")
    func modesAreDescribed() {
        #expect(LedMode.all.allSatisfy { !$0.desc.isEmpty })
    }

    /// Every mode gets a preview, and no mode gets a preview that pretends
    /// to more than was observed. `rgb` is the one whose pattern was never
    /// captured off the wire, so it -- and only it -- is flagged approximate
    /// and the pane prints a caveat under it.
    @Test("each mode previews as something, and only the uncaptured one is flagged")
    func previewsAreHonest() {
        let approximate = LedMode.all.filter { $0.appearance.isApproximate }.map(\.id)
        #expect(approximate == ["rgb"], "\(approximate)")
        #expect(LedMode.named("off")?.appearance == .unlit)
        #expect(LedMode.named("rainbow")?.appearance == .palette(RGB.vendorRainbow))
    }

    /// The two fixed-colour modes must preview in their own colours. Drawing
    /// mode 2 red would put the app back to the reading that produced the
    /// wrong names in the first place -- that the knob has one colour.
    @Test("the fixed-colour modes preview in different colours")
    func fixedColourModesDiffer() {
        guard case .steady(let red)? = LedMode.named("red")?.appearance,
              case .steady(let green)? = LedMode.named("green")?.appearance else {
            Issue.record("the fixed-colour modes are not steady colours")
            return
        }
        #expect(red != green)
        #expect(red.red > red.green && red.red > red.blue, "mode 1 is not red: \(red)")
        #expect(green.green > green.red && green.green > green.blue, "mode 2 is not green: \(green)")
    }

    /// The vendor's rainbow, in the vendor's order. Six entries, starting at
    /// red -- the palette captured off ANTICATER.app's own wire traffic.
    @Test("the rainbow preview uses the captured palette")
    func rainbowUsesTheCapturedPalette() {
        #expect(RGB.vendorRainbow.count == 6)
        #expect(RGB.vendorRainbow.first == RGB(1.0, 0.0, 0.0))
        #expect(Set(RGB.vendorRainbow).count == 6, "the palette repeats an entry")
    }
}

@Suite("Device layer binding")
struct DeviceBindingPresentationTests {
    /// The arrangement is a separate claim from the mode. `mode` says
    /// whether the knob sends slot chords at all; the binding says which of
    /// the three firmware layers carries them, and therefore which run
    /// standalone with the daemon stopped. The app used to present all three
    /// host layers as though the daemon drove all three.
    @Test func theBindingIsCarriedAlongsideTheModeNotInsteadOfIt() {
        let p = ModePresentation(
            rawMode: "host-translate",
            deviceBinding: "Device layer 2 is host-translated; layer(s) 1, 3 run standalone."
        )
        #expect(p.hostLayersCanFire)
        #expect(p.deviceBinding?.contains("layer 2") == true)
    }

    /// A blank summary is not a summary. Rendering one would put an empty
    /// row where an explanation belongs.
    @Test func anEmptyOrMissingSummaryIsTreatedAsAbsent() {
        #expect(ModePresentation(rawMode: "host-translate").deviceBinding == nil)
        #expect(ModePresentation(rawMode: "host-translate", deviceBinding: "").deviceBinding == nil)
        #expect(
            ModePresentation(rawMode: "host-translate", deviceBinding: "   \n ").deviceBinding == nil
        )
    }

    /// Whitespace around a real summary is trimmed, not treated as absent.
    @Test func aPaddedSummaryIsKept() {
        let p = ModePresentation(rawMode: "standalone", deviceBinding: "  bound to layer 1  ")
        #expect(p.deviceBinding == "bound to layer 1")
    }

    /// Both claims can be present at once: the knob is standalone AND
    /// nothing is recorded as bound. Neither message may suppress the other.
    @Test func aStandaloneKnobStillReportsItsRecordedArrangement() {
        let p = ModePresentation(
            rawMode: "standalone",
            deviceBinding: "No device layer is bound to slot chords."
        )
        #expect(!p.hostLayersCanFire)
        #expect(p.deviceBinding != nil)
    }
}

/// The daemon describes its own commands; the pane renders that description.
/// The pane used to carry a hand-written list of eleven against seventeen
/// real ones, naming a tool that does not exist and three parameters by the
/// wrong name.
@Suite("Daemon command table")
struct DaemonCommandTests {
    private func payload(
        name: String = "set_led",
        mcp: Bool = true,
        cliReason: String? = nil
    ) -> [String: Any] {
        var cli: [String: Any] = ["available": cliReason == nil]
        if let cliReason { cli["reason"] = cliReason }
        return [
            "name": name,
            "about": "Set a layer's backlight mode.",
            "cli_name": "led",
            "cli": cli,
            "mcp": ["available": mcp],
            "params": [
                ["name": "layer", "kind": "integer", "about": "Device layer 0-2", "required": true],
                ["name": "mode", "kind": "string", "about": "Mode name", "required": true]
            ]
        ]
    }

    @Test("a command decodes with its parameters and their required-ness")
    func decodesACommand() {
        let cmd = DaemonCommand(json: payload())
        #expect(cmd?.name == "set_led")
        #expect(cmd?.cliName == "led")
        #expect(cmd?.params.map(\.name) == ["layer", "mode"])
        #expect(cmd?.params.allSatisfy(\.required) == true)
        #expect(cmd?.params.first?.signature == "layer: integer")
    }

    /// An optional parameter must read as optional. The hand-written list
    /// appended "?" to whichever ones its author remembered, which is how
    /// `set_led` came to advertise a `color` its schema does not carry.
    @Test("an optional parameter is marked optional")
    func optionalParamsAreMarked() {
        var json = payload()
        json["params"] = [["name": "layer", "kind": "integer", "about": "", "required": false]]
        #expect(DaemonCommand(json: json)?.params.first?.signature == "layer: integer?")
    }

    /// The registry records WHY a command is absent from a surface. Dropping
    /// the reason makes a considered omission look like an oversight.
    @Test("the stated reason for a missing surface is carried through")
    func absenceCarriesItsReason() {
        let cmd = DaemonCommand(json: payload(cliReason: "acts on a running daemon's engine"))
        #expect(cmd?.cli.available == false)
        #expect(cmd?.cli.reason == "acts on a running daemon's engine")
    }

    @Test("only the commands a caller can actually reach are listed")
    func callableFiltersBySurface() {
        let all = [
            DaemonCommand(json: payload(name: "set_led", mcp: true)),
            DaemonCommand(json: payload(name: "validate", mcp: false))
        ].compactMap { $0 }
        #expect(DaemonCommand.callable(all).map(\.name) == ["set_led"])
    }

    @Test("a payload without a name is dropped rather than half-decoded")
    func namelessPayloadIsRejected() {
        #expect(DaemonCommand(json: ["about": "no name here"]) == nil)
    }
}

/// The command list shows a scannable line and keeps the rest on hover.
///
/// These `about` strings are MCP tool descriptions, written for an agent
/// reading a schema. `bind_sequence` runs to four lines about slot capacity
/// and chord cost. Right where an agent reads it; wrong in a list of
/// eighteen rows someone is scanning.
@Suite("Command summaries")
struct DaemonCommandSummaryTests {
    private func cmd(_ about: String) -> DaemonCommand? {
        DaemonCommand(json: ["name": "x", "about": about])
    }

    @Test("a multi-sentence description shows its first sentence")
    func firstSentenceOnly() {
        let c = cmd("Set a layer's backlight mode. Modes: off, red, green, ripple.")
        #expect(c?.summary == "Set a layer's backlight mode.")
        #expect(c?.hasMoreDetail == true)
    }

    /// A one-sentence description is shown whole, and must not be flagged as
    /// having more behind it -- an affordance for detail that opens nothing
    /// is the defect this whole sweep is about.
    @Test("a single sentence is shown in full and flags no hidden detail")
    func singleSentenceIsWhole() {
        let c = cmd("Read back a layer's current LED mode. ")
        #expect(c?.summary == "Read back a layer's current LED mode.")
        #expect(c?.hasMoreDetail == false)
    }

    /// Splitting on "." alone would cut these in half. The registry is full
    /// of them: hex constants, "e.g.", version numbers.
    @Test("a period inside a sentence does not end it")
    func innerPeriodsDoNotSplit() {
        let hex = cmd("Opens the vendor endpoint 0xFF00.5 and reads it. Then stops.")
        #expect(hex?.summary == "Opens the vendor endpoint 0xFF00.5 and reads it.")
        let eg = cmd("Takes a name, e.g. volumeup, and binds it.")
        #expect(eg?.summary == "Takes a name, e.g. volumeup, and binds it.")
        #expect(eg?.hasMoreDetail == false)
    }

    @Test("an empty description degrades to empty rather than crashing")
    func emptyIsSafe() {
        #expect(cmd("")?.summary == "")
        #expect(cmd("")?.hasMoreDetail == false)
    }
}
