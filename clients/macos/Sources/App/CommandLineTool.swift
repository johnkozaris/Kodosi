import Foundation

enum CommandLineTool {
    private static let elsewhere = ["/opt/homebrew/bin/kodosi", "/usr/local/bin/kodosi", "~/bin/kodosi"]
    static let link = "~/.local/bin/kodosi"

    static var installed: Bool {
        let path = Bundle.main.bundlePath
        return path.hasPrefix("/Applications/") || path.hasPrefix(FileManager.default.homeDirectoryForCurrentUser.path + "/Applications/")
    }

    /// Homebrew folders stay Homebrew's: the cask makes its own link there.
    @discardableResult
    static func keepLinked(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> String? {
        guard let own = Bundle.main.executableURL?.resolvingSymlinksInPath() else { return nil }
        let expand = { (path: String) in URL(fileURLWithPath: path.replacingOccurrences(of: "~", with: home.path, options: .anchored)) }
        if let linked = ([link] + elsewhere).first(where: { expand($0).resolvingSymlinksInPath() == own }) {
            return linked
        }
        let target = expand(link)
        let files = FileManager.default
        do {
            try files.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let destination = try? files.destinationOfSymbolicLink(atPath: target.path) {
                guard destination.contains(".app/Contents/MacOS/Kodosi") else { return nil }
                try files.removeItem(at: target)
            } else if files.fileExists(atPath: target.path) {
                return nil
            }
            try files.createSymbolicLink(at: target, withDestinationURL: own)
            return link
        } catch {
            return nil
        }
    }
}
