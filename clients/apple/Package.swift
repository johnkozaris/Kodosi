// swift-tools-version: 6.4

import PackageDescription

let swiftSettings: [SwiftSetting] = [
    .swiftLanguageMode(.v6),
    .enableUpcomingFeature("MemberImportVisibility"),
    .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
    .enableUpcomingFeature("InferIsolatedConformances"),
    .enableUpcomingFeature("ExistentialAny"),
    .enableUpcomingFeature("ImmutableWeakCaptures"),
]

let package = Package(
    name: "KodosiApple",
    defaultLocalization: "en",
    platforms: [
        .macOS("26.0"),
        .iOS("26.0"),
    ],
    products: [
        .library(name: "KodosiKit", targets: ["KodosiKit"]),
        .library(name: "KodosiBridge", targets: ["KodosiBridge"]),
        .library(name: "KodosiDesign", targets: ["KodosiDesign"]),
        .library(name: "KodosiLinks", targets: ["KodosiLinks"]),
        .library(name: "KodosiTerminal", targets: ["KodosiTerminal"]),
        .library(name: "KodosiTerminalSessions", targets: ["KodosiTerminalSessions"]),
    ],
    dependencies: [
        .package(path: "../../terminal/ghostty"),
    ],
    targets: [
        .systemLibrary(name: "KodosiKit", path: "Sources/KodosiKit"),
        .target(
            name: "KodosiBridge",
            dependencies: ["KodosiKit"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .target(
            name: "KodosiLinks",
            dependencies: ["KodosiKit"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .target(
            name: "KodosiDesign",
            dependencies: ["KodosiBridge", "KodosiLinks"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .target(
            name: "KodosiTerminal",
            dependencies: [
                "KodosiKit",
                "KodosiLinks",
                .product(name: "GhosttyTerminal", package: "ghostty"),
            ],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .target(
            name: "KodosiTerminalSessions",
            dependencies: ["KodosiKit", "KodosiBridge", "KodosiTerminal"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .testTarget(
            name: "KodosiTerminalTests",
            dependencies: [
                "KodosiTerminal",
                "KodosiLinks",
                .product(name: "GhosttyTerminal", package: "ghostty"),
            ],
            swiftSettings: swiftSettings,
            linkerSettings: [
                .unsafeFlags(["-Xlinker", "-undefined", "-Xlinker", "dynamic_lookup"]),
            ],
        ),
        .testTarget(
            name: "KodosiBridgeTests",
            dependencies: ["KodosiBridge", "KodosiKit"],
            swiftSettings: swiftSettings,
            linkerSettings: [
                // The tests link the bridge without the runtime library. No test calls its functions.
                .unsafeFlags(["-Xlinker", "-undefined", "-Xlinker", "dynamic_lookup"]),
            ],
        ),
        .testTarget(
            name: "KodosiLinksTests",
            dependencies: ["KodosiLinks"],
            swiftSettings: swiftSettings,
            linkerSettings: [
                // The link rule is the runtime's, which the Mac app's tests run with. These test only
                // what is this app's own part.
                .unsafeFlags(["-Xlinker", "-undefined", "-Xlinker", "dynamic_lookup"]),
            ],
        ),
    ],
)
