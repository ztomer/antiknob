// swift-tools-version: 6.0
import PackageDescription

// Antiknob.app's Swift half, as a package so the house Swift gate can reach
// it: swiftlint --strict, a cold warnings-as-errors build, and `swift test`.
// Before this it was a hand-rolled `swiftc` line in build.sh, which meant the
// gate suite covered the Rust half thoroughly and this half with nothing.
//
// The split is drawn where testability changes, not arbitrarily:
//   AntiknobUI  -- every view, the config store, the models and the socket
//                  client. Tests reach it with `@testable import`, so nothing
//                  here needs a `public` annotation it would not otherwise have.
//   Antiknob    -- the `@main` entry point and nothing else, because `@main`
//                  cannot live in a library.
let package = Package(
    name: "Antiknob",
    platforms: [.macOS("26.0")],
    targets: [
        .target(
            name: "AntiknobUI",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .executableTarget(
            name: "Antiknob",
            dependencies: ["AntiknobUI"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "AntiknobUITests",
            dependencies: ["AntiknobUI"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        )
    ]
)
