import Foundation
import PackagePlugin

@main
struct VerifyGhosttyPlugin: BuildToolPlugin {
    func createBuildCommands(context: PluginContext, target _: Target) throws -> [Command] {
        let repository = context.package.directoryURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let verifier = repository.appending(path: "scripts/build/verify-ghostty-checkout.sh")
        let output = context.pluginWorkDirectoryURL.appending(path: "verified")
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        var environment: [String: String] = [:]
        if ProcessInfo.processInfo.environment["KODOSI_WORKING_TREE_VALIDATION"] == "1" {
            environment["KODOSI_WORKING_TREE_VALIDATION"] = "1"
        }
        return [
            .prebuildCommand(
                displayName: "Verify locked Ghostty checkout",
                executable: URL(filePath: "/bin/bash"),
                arguments: [verifier.path],
                environment: environment,
                outputFilesDirectory: output
            ),
        ]
    }
}
