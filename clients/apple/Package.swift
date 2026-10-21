// swift-tools-version: 6.3

import PackageDescription

let swiftSettings: [SwiftSetting] = [
    .swiftLanguageMode(.v6),
    .enableUpcomingFeature("MemberImportVisibility"),
    .enableUpcomingFeature("NonisolatedNonsendingByDefault"),
    .enableUpcomingFeature("InferIsolatedConformances"),
]

let package = Package(
    name: "KodosiApple",
    defaultLocalization: "en",
    platforms: [
        .macOS("26.4"),
        .iOS("26.0"),
    ],
    products: [
        .library(name: "KodosiKit", targets: ["KodosiKit"]),
        .library(name: "KodosiBridge", targets: ["KodosiBridge"]),
        .library(name: "KodosiDesign", targets: ["KodosiDesign"]),
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
            name: "KodosiDesign",
            dependencies: ["KodosiBridge"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .target(
            name: "KodosiTerminal",
            dependencies: [
                "KodosiKit",
                .product(name: "GhosttyTerminal", package: "ghostty"),
            ],
            swiftSettings: swiftSettings,
            plugins: [
                .plugin(name: "VerifyGhosttyPlugin"),
            ],
        ),
        .target(
            name: "KodosiTerminalSessions",
            dependencies: ["KodosiKit", "KodosiBridge", "KodosiTerminal"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings,
        ),
        .testTarget(
            name: "KodosiTerminalTests",
            dependencies: ["KodosiTerminal"],
            swiftSettings: [
                .swiftLanguageMode(.v6),
            ],
        ),
        .plugin(
            name: "VerifyGhosttyPlugin",
            capability: .buildTool(),
        ),
    ],
)
