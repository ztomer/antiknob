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

    /// The inline chain this replaced fell through to "Press" for anything
    /// above 3, so mode 5 rendered as "Press (Reactive)".
    @Test("custom mode is not mislabelled as press")
    func customIsNotPress() {
        #expect(ledModeNames[5] == "Custom")
        #expect(ledModeNames[4] == "Press (Reactive)")
    }

    @Test("a mode number the firmware should never send has no name")
    func unknownModeIsUnmapped() {
        #expect(ledModeNames[9] == nil)
    }
}
