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

@Suite("LED mode names")
struct LedModeNameTests {
    @Test("every mode the firmware reports has its own name")
    func modesAreNamedAndDistinct() {
        let names = (0...5).map { ledModeNames[$0] }
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
        #expect(ledModeNames[1] == "Red")
        #expect(ledModeNames[2] == "Green")
        #expect(ledModeNames[4] == "Rainbow")
        #expect(ledModeNames[5] == "RGB")
        for stale in ["Backlight", "Shock (Breathe)", "Shock 2 (Rapid)", "Press (Reactive)"] {
            #expect(!ledModeNames.values.contains(stale), "884x name survives: \(stale)")
        }
    }

    @Test("a mode number the firmware should never send has no name")
    func unknownModeIsUnmapped() {
        #expect(ledModeNames[9] == nil)
    }
}

/// The layer view must never present a host layer as live when the firmware
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

    @Test("host-translate says nothing, because nothing is wrong")
    func hostTranslateIsQuiet() {
        let p = ModePresentation(rawMode: "host-translate")
        #expect(p.banner == nil)
        #expect(p.callToAction == nil)
    }

    @Test("standalone says the layers are inactive and offers the way out")
    func standaloneExplainsItself() {
        let p = ModePresentation(rawMode: "standalone")
        #expect(p.mode == .standalone)
        #expect(p.banner?.contains("inactive") == true, "banner: \(p.banner ?? "nil")")
        #expect(p.callToAction == "Flash Slot Bindings")
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
            #expect(p.banner != nil)
            #expect(p.callToAction == nil)
        }
    }

    @Test("each mode has its own glyph")
    func glyphsAreDistinct() {
        let icons = ["host-translate", "standalone", "unknown"]
            .map { ModePresentation(rawMode: $0).icon }
        #expect(Set(icons).count == 3, "two modes share a glyph: \(icons)")
    }
}

/// The LED controls must describe this device's own modes, and must not
/// offer a promise the firmware does not keep.
@Suite("LED mode list")
struct LedModeTests {
    @Test("the modes are the 8850's, not the 884x names")
    func modesMatchTheDevice() {
        let ids = LedMode.all.map(\.id)
        #expect(ids == ["red", "green", "ripple", "rainbow", "rgb", "off"], "\(ids)")
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

    @Test("the colour notice says colour is ignored, not that it is broken")
    func colourNoticeIsHonest() {
        let n = LedMode.colourNotice
        #expect(n.contains("ignored"), "\(n)")
        #expect(n.contains("effect"), "\(n)")
    }

    @Test("every mode has a description and its own glyph")
    func modesAreDistinct() {
        #expect(LedMode.all.allSatisfy { !$0.desc.isEmpty })
        let icons = LedMode.all.map(\.icon)
        #expect(Set(icons).count == icons.count, "two modes share a glyph: \(icons)")
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
        // Nothing is wrong, so there is still no warning banner.
        #expect(p.banner == nil)
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
        #expect(p.banner != nil)
        #expect(p.deviceBinding != nil)
        #expect(p.callToAction == "Flash Slot Bindings")
    }
}
