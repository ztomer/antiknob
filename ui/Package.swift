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
//
// Inside AntiknobUI, `Core/` holds the sources with no view declarations at
// all -- models, the socket client, the parsers and the presentation values.
// It is a DIRECTORY and not a target on purpose. A separate target would put
// a module boundary between the views and the types they are built from, and
// every one of those types, its members and its initialisers would need
// `public` for no reason but the boundary. The only thing the split was ever
// wanted for is a coverage floor aimed at code unit tests actually execute,
// and coverage-floors.json aims one at the directory (46%) while the package
// floor stays where the package as a whole is (4%). Letting a coverage tool
// dictate a package's module structure is the wrong way round.
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
