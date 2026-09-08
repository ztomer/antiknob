// Tests for the device status parser.
//
// This logic was duplicated between the daemon and CLI branches of
// `ConfigStore.refreshStatus()` and had never been exercised: the function
// launches a subprocess and lives on a store that cannot be constructed in
// a test. It decides what the hardware banner claims, which makes an
// unchecked fallback here a UI that states things about a device it did not
// hear from.

import Foundation
import Testing

@testable import AntiknobUI

@Suite("Status parsing")
struct StatusParseTests {
    @Test func anEmptyPayloadClaimsNothing() {
        let parsed = StatusParse.parse([:])
        #expect(parsed.hardwareFound == false)
        #expect(parsed.product == "Not detected")
        #expect(parsed.transport == "disconnected")
        #expect(parsed.devices.isEmpty)
        #expect(parsed.tapActive == false)
        #expect(parsed.tapError == nil)
    }

    /// "Not detected" and a model name are different claims. A device that
    /// is absent must never be described by the name of a device.
    @Test func anAbsentDeviceIsNeverGivenAModelName() {
        for payload in [[:], ["devices": []] as [String: Any]] {
            let parsed = StatusParse.parse(payload)
            #expect(parsed.hardwareFound == false)
            #expect(parsed.product == "Not detected")
            #expect(parsed.product != StatusParse.unnamedDevice)
        }
    }

    @Test func aPresentDeviceIsNamedByItsOwnName() {
        let parsed = StatusParse.parse([
            "devices": [["name": "Anticater / LQKJ VK01"]]
        ])
        #expect(parsed.hardwareFound)
        #expect(parsed.product == "Anticater / LQKJ VK01")
        #expect(parsed.devices.count == 1)
    }

    /// The fallback chain, including the empty-string steps -- a device
    /// reporting `"name": ""` would otherwise blank the banner.
    @Test func theProductNameFallsThroughEmptyValues() {
        #expect(StatusParse.productName(of: ["name": "A", "product_string": "B"]) == "A")
        #expect(StatusParse.productName(of: ["name": "", "product_string": "B"]) == "B")
        #expect(StatusParse.productName(of: ["name": "", "product_string": ""])
            == StatusParse.unnamedDevice)
        #expect(StatusParse.productName(of: [:]) == StatusParse.unnamedDevice)
        // A non-string value must not crash or stringify into the banner.
        #expect(StatusParse.productName(of: ["name": 42]) == StatusParse.unnamedDevice)
    }

    /// The top-level transport is the daemon's own answer; the power
    /// block's copy is a restatement. If the copy could win, a stale power
    /// reading would be able to contradict a fresh transport one.
    @Test func theTopLevelTransportOutranksThePowerBlocksCopy() {
        let parsed = StatusParse.parse([
            "transport": "usb",
            "power": ["transport": "bluetooth", "description": "Bluetooth Wireless"]
        ])
        #expect(parsed.transport == "usb")
        #expect(parsed.powerDescription == "Bluetooth Wireless")
    }

    /// ...but it is a real fallback when the top level says nothing.
    @Test func thePowerBlockSuppliesTheTransportWhenTheTopLevelDoesNot() {
        let parsed = StatusParse.parse(["power": ["transport": "wireless_2_4g"]])
        #expect(parsed.transport == "wireless_2_4g")
    }

    @Test func tapStateIsReadWhenPresentAndDefaultedWhenNot() {
        let active = StatusParse.parse(["tap_active": true, "tap_error": "denied"])
        #expect(active.tapActive)
        #expect(active.tapError == "denied")
        // The CLI payload carries no tap keys at all: a tool with no event
        // tap cannot report one as failed, so absent means quiet, not error.
        let cli = StatusParse.parse(["devices": [["name": "VK01"]]])
        #expect(cli.tapActive == false)
        #expect(cli.tapError == nil)
    }

    /// A reply missing some fields degrades field by field rather than
    /// being thrown away wholesale.
    @Test func aPartialPayloadKeepsWhatItDidCarry() {
        let parsed = StatusParse.parse([
            "transport": "usb",
            "devices": [["product_string": "VK01"]]
        ])
        #expect(parsed.transport == "usb")
        #expect(parsed.product == "VK01")
        #expect(parsed.hardwareFound)
        // Untouched fields keep their documented defaults.
        #expect(parsed.powerDescription == "Wired (USB Bus Powered)")
    }

    /// Wrongly-typed values must fall back, not crash and not leak through.
    @Test func wronglyTypedFieldsFallBackToTheDefaults() {
        let parsed = StatusParse.parse([
            "transport": 7,
            "tap_active": "yes",
            "power": "not a dictionary",
            "devices": "not a list"
        ])
        #expect(parsed.transport == "disconnected")
        #expect(parsed.tapActive == false)
        #expect(parsed.powerDescription == "Wired (USB Bus Powered)")
        #expect(parsed.hardwareFound == false)
    }
}

/// The device the pane NAMES must be the device the pane TALKS TO.
///
/// A knob publishes several HID interfaces. `devices[0]` is whichever
/// enumerated first, and on the hardware this was found on that is a
/// `0x514c:0x4155` keyboard endpoint sitting four rows above the
/// `0x514c:0x8850` knob every button on the pane drives. The daemon now
/// names the vendor configuration endpoint it opens, and this reads that.
@Suite("Primary device")
struct PrimaryDeviceParseTests {
    private let keyboardFirst: [[String: Any]] = [
        ["name": "Anticater / LiQi (0x514c:0x4155)", "usage_page": 1],
        ["name": "Anticater / LQKJ VK01 (0x514c:0x8850)", "usage_page": 1]
    ]

    @Test("the daemon's chosen endpoint wins over whatever enumerated first")
    func primaryDeviceIsPreferred() {
        let parsed = StatusParse.parse([
            "devices": keyboardFirst,
            "primary_device": [
                "name": "Anticater / LQKJ VK01 (0x514c:0x8850)",
                "usage_page": 0xFF00
            ]
        ])
        #expect(parsed.product == "Anticater / LQKJ VK01 (0x514c:0x8850)")
        #expect(parsed.hardwareFound)
        // The full list is still carried; only the headline changed.
        #expect(parsed.devices.count == 2)
    }

    /// An older daemon does not send the key. Falling back to the first
    /// entry is what this build did everywhere, so it stays the fallback
    /// rather than becoming "unknown".
    @Test("a payload without a primary falls back to the first endpoint")
    func fallsBackToFirstEndpoint() {
        let parsed = StatusParse.parse(["devices": keyboardFirst])
        #expect(parsed.product == "Anticater / LiQi (0x514c:0x4155)")
    }

    /// The daemon's active host layer, so the menu bar's checkmark tracks
    /// the daemon rather than the last thing clicked in this process.
    @Test("the active layer is read, and its absence is not read as zero")
    func activeLayerIsCarried() {
        #expect(StatusParse.parse(["active_layer": 2]).activeLayer == 2)
        // The CLI's status has no engine to ask. Defaulting to 0 would put a
        // checkmark on a layer nobody selected.
        #expect(StatusParse.parse([:]).activeLayer == nil)
    }
}
