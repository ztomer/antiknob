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
