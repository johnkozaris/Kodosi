// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "KodosiTerminal",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .library(
            name: "KodosiTerminal",
            targets: ["KodosiTerminal"]
        ),
    ],
    dependencies: [
        .package(path: "../../../../terminal/ghostty"),
    ],
    targets: [
        .target(
            name: "KodosiTerminal",
            dependencies: [
                .product(
                    name: "GhosttyTerminal",
                    package: "ghostty"
                ),
            ],
            swiftSettings: [
                .swiftLanguageMode(.v6),
            ],
            plugins: [
                .plugin(name: "VerifyGhosttyPlugin"),
            ]
        ),
        .testTarget(
            name: "KodosiTerminalTests",
            dependencies: ["KodosiTerminal"]
        ),
        .plugin(
            name: "VerifyGhosttyPlugin",
            capability: .buildTool()
        ),
    ]
)
