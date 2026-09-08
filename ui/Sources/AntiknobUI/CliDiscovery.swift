// CliDiscovery.swift — finding the `antiknob` binary, and reading what it says.
//
// Both halves used to live inline in `ConfigStore.queryCliStatus()`, a
// `Process`-launching function no test can call: it shells out, and the
// store that holds it cannot even be constructed without loading config and
// starting a 2s timer. So the two decisions it makes -- WHERE to look for
// the binary, and WHAT its JSON means -- had no coverage at all, in a
// package whose measured figure is 3%.
//
// Neither decision needs a process. Pulling them out here makes both pure
// value transforms with the filesystem behind a closure, which is the whole
// of the seam: `ConfigStore` still launches the binary, but it no longer
// owns the reasoning about it.

import Foundation

public enum CliDiscovery {
    /// Where the `antiknob` binary might be, in the order to prefer them.
    ///
    /// Order is the point. The installed copy under `/Applications` wins
    /// over a `~/.cargo/bin` build, so a developer's stale debug binary
    /// cannot shadow the one the app shipped with -- and the copy sitting
    /// beside the app bundle comes last, because it is the fallback for an
    /// app run out of a build directory, not a normal install.
    public static func searchPaths(
        home: String,
        resourcePath: String?,
        besideApp: String
    ) -> [String] {
        var paths = [
            "/Applications/Antiknob/bin/antiknob",
            "\(home)/.local/bin/antiknob",
            "\(home)/.cargo/bin/antiknob",
            "/usr/local/bin/antiknob"
        ]
        if let resourcePath {
            paths.append("\(resourcePath)/antiknob")
        }
        paths.append(besideApp)
        return paths
    }

    /// The first candidate that is actually runnable, or nil.
    ///
    /// `isExecutable` is a parameter so the choice can be tested without a
    /// filesystem that happens to have an `antiknob` in one of these places
    /// -- a test that passes only on a machine with the tool installed is
    /// a test that says nothing on the machine that lacks it.
    public static func resolve(
        paths: [String],
        isExecutable: (String) -> Bool
    ) -> String? {
        paths.first(where: isExecutable)
    }

    /// Make sense of whatever `antiknob status --json` printed.
    ///
    /// Two shapes have to be accepted. The object form is what the CLI
    /// emits now. The bare-array form is an older one, and it carries no
    /// `connected` flag at all -- so this synthesises it from whether the
    /// array has anything in it, which is the only honest reading: a list
    /// of zero devices means nothing is attached.
    ///
    /// Anything else is nil rather than a default-shaped dictionary. A
    /// status that could not be read is not a status saying "disconnected";
    /// reporting one as the other is how a UI comes to state a fact about
    /// hardware it never heard from.
    public static func decodeStatus(_ data: Data) -> [String: Any]? {
        guard let parsed = try? JSONSerialization.jsonObject(with: data) else {
            return nil
        }
        if let object = parsed as? [String: Any] {
            return object
        }
        if let devices = parsed as? [[String: Any]] {
            return [
                "connected": !devices.isEmpty,
                "devices": devices,
                "product_string": devices.first?["name"] as? String ?? "Anticater VK01"
            ]
        }
        return nil
    }
}
