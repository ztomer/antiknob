// Tests for finding the `antiknob` binary and reading its status output.
//
// This logic used to sit inside a `Process`-launching method on a store that
// cannot be constructed in a test, so none of it had ever been exercised.
// The decisions it makes are not incidental: the search ORDER decides
// whether a developer's stale build shadows the shipped binary, and the
// status decoder decides whether "could not read the hardware" is reported
// as "no hardware".

import Foundation
import Testing

@testable import AntiknobUI

@Suite("CLI discovery")
struct CliDiscoveryTests {
    /// The installed copy has to beat a local build, or an old debug binary
    /// in `~/.cargo/bin` silently becomes the one the app talks to.
    @Test func theInstalledBinaryOutranksALocalBuild() {
        let paths = CliDiscovery.searchPaths(
            home: "/Users/x",
            resourcePath: "/App.app/Contents/Resources",
            besideApp: "/somewhere/antiknob"
        )
        let installed = paths.firstIndex(of: "/Applications/Antiknob/bin/antiknob")
        let cargo = paths.firstIndex(of: "/Users/x/.cargo/bin/antiknob")
        let local = paths.firstIndex(of: "/Users/x/.local/bin/antiknob")
        #expect(installed != nil)
        #expect(cargo != nil)
        #expect(installed! < local!)
        #expect(local! < cargo!)
        // The copy beside the bundle is the last resort, not the first.
        #expect(paths.last == "/somewhere/antiknob")
    }

    /// An app with no resource path still gets a usable list; the bundled
    /// candidate is simply absent rather than a path with "nil" in it.
    @Test func aMissingResourcePathDropsThatCandidateOnly() {
        let withRes = CliDiscovery.searchPaths(
            home: "/Users/x", resourcePath: "/R", besideApp: "/b/antiknob")
        let without = CliDiscovery.searchPaths(
            home: "/Users/x", resourcePath: nil, besideApp: "/b/antiknob")
        #expect(withRes.count == without.count + 1)
        #expect(withRes.contains("/R/antiknob"))
        #expect(!without.contains(where: { $0.contains("nil") }))
        #expect(without.last == "/b/antiknob")
    }

    /// Resolution takes the first RUNNABLE candidate, not the first listed.
    @Test func resolutionSkipsCandidatesThatAreNotExecutable() {
        let paths = ["/a/antiknob", "/b/antiknob", "/c/antiknob"]
        #expect(CliDiscovery.resolve(paths: paths, isExecutable: { $0 == "/b/antiknob" })
            == "/b/antiknob")
        // Present in more than one place: the earlier one wins.
        #expect(CliDiscovery.resolve(paths: paths, isExecutable: { $0 != "/a/antiknob" })
            == "/b/antiknob")
        // Nothing runnable is nil, not a path that does not exist.
        #expect(CliDiscovery.resolve(paths: paths, isExecutable: { _ in false }) == nil)
        #expect(CliDiscovery.resolve(paths: [], isExecutable: { _ in true }) == nil)
    }

    @Test func theObjectFormIsPassedThroughUnchanged() {
        let data = Data(#"{"connected":true,"transport":"usb"}"#.utf8)
        let decoded = CliDiscovery.decodeStatus(data)
        #expect(decoded?["connected"] as? Bool == true)
        #expect(decoded?["transport"] as? String == "usb")
    }

    /// The older array form carries no `connected` flag, so one is derived.
    /// A non-empty list means something is attached; an empty one means it
    /// is not. Getting this backwards would light the UI up for no device.
    @Test func theArrayFormDerivesConnectednessFromTheDeviceCount() {
        let populated = Data(#"[{"name":"Anticater / LQKJ VK01"}]"#.utf8)
        let decoded = CliDiscovery.decodeStatus(populated)
        #expect(decoded?["connected"] as? Bool == true)
        #expect(decoded?["product_string"] as? String == "Anticater / LQKJ VK01")
        #expect((decoded?["devices"] as? [[String: Any]])?.count == 1)

        let empty = CliDiscovery.decodeStatus(Data("[]".utf8))
        #expect(empty?["connected"] as? Bool == false)
        #expect((empty?["devices"] as? [[String: Any]])?.isEmpty == true)
    }

    /// A device with no name still yields a usable product string rather
    /// than an empty banner.
    @Test func anUnnamedDeviceFallsBackToTheModelName() {
        let decoded = CliDiscovery.decodeStatus(Data(#"[{"vendor_id":20812}]"#.utf8))
        #expect(decoded?["product_string"] as? String == "Anticater VK01")
        #expect(decoded?["connected"] as? Bool == true)
    }

    /// The property that matters most: unreadable output is NOT a status
    /// saying "disconnected". A UI that renders the two the same way states
    /// a fact about hardware it never heard from.
    @Test func unreadableOutputIsNilRatherThanASyntheticDisconnectedStatus() {
        for junk in ["", "not json at all", "{", "[1,2,3]", "\"a string\"", "null"] {
            #expect(
                CliDiscovery.decodeStatus(Data(junk.utf8)) == nil,
                "\(junk.debugDescription) must not decode to a status"
            )
        }
    }
}
