import Foundation
import PackagePlugin

@main
struct VerifyGhosttyPlugin: BuildToolPlugin {
    func createBuildCommands(context: PluginContext, target _: Target) throws -> [Command] {
        let repository = context.package.directoryURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let verifier = repository.appending(path: "terminal/ghostty/scripts/verify-ghostty-pins.py")
        let output = context.pluginWorkDirectoryURL.appending(path: "verified")
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        var arguments = ["python3", verifier.path]
        if ProcessInfo.processInfo.environment["KODOSI_WORKING_TREE_VALIDATION"] == "1" {
            arguments.append("--working-tree")
        }
        return [
            .prebuildCommand(
                displayName: "Verify the Ghostty pins",
                executable: URL(filePath: "/usr/bin/env"),
                arguments: arguments,
                outputFilesDirectory: output
            ),
        ]
    }
}
