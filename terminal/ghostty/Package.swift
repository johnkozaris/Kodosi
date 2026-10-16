// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "KodosiGhostty",
    platforms: [
        .macOS(.v13),
        .iOS("26.0"),
    ],
    products: [
        .library(name: "GhosttyTerminal", targets: ["GhosttyTerminal"]),
    ],
    targets: [
        .target(
            name: "GhosttyKit",
            dependencies: ["libghostty"],
            path: "Sources/GhosttyKit",
            linkerSettings: [
                .linkedLibrary("c++"),
                .linkedFramework("Carbon", .when(platforms: [.macOS])),
                .linkedFramework("CoreFoundation", .when(platforms: [.iOS])),
                .linkedFramework("CoreGraphics", .when(platforms: [.iOS])),
                .linkedFramework("CoreText", .when(platforms: [.iOS])),
                .linkedFramework("IOSurface", .when(platforms: [.iOS])),
                .linkedFramework("Metal", .when(platforms: [.iOS])),
                .linkedFramework("QuartzCore", .when(platforms: [.iOS])),
            ]
        ),
        .target(
            name: "GhosttyTerminal",
            dependencies: ["GhosttyKit"],
            path: "Sources/GhosttyTerminal"
        ),
        .binaryTarget(
            name: "libghostty",
            path: "Vendor/GhosttyKit.xcframework"
        ),
        .testTarget(
            name: "GhosttyKitTest",
            dependencies: ["GhosttyKit", "GhosttyTerminal"],
            resources: [.copy("Resources/visual-checkpoint.snapshot")]
        ),
    ]
)
