// swift-tools-version: 6.0

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
    ],
    targets: [
        .systemLibrary(name: "KodosiKit", path: "Sources/KodosiKit"),
        .target(
            name: "KodosiBridge",
            dependencies: ["KodosiKit"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings
        ),
        .target(
            name: "KodosiDesign",
            dependencies: ["KodosiBridge"],
            resources: [.process("Resources")],
            swiftSettings: swiftSettings
        ),
    ]
)
