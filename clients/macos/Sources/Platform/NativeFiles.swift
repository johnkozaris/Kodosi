import AppKit
import Foundation

enum NativeFiles {
    @MainActor
    static func open(_ path: String) throws {
        guard path.hasPrefix("/"), path.utf8.count <= 4096,
              !path.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
        else { throw RuntimeError.operation(String(localized: "The local path is invalid.")) }
        let url = URL(fileURLWithPath: path).standardizedFileURL.resolvingSymlinksInPath()
        guard FileManager.default.fileExists(atPath: url.path), FileManager.default.isReadableFile(atPath: url.path) else {
            throw RuntimeError.operation(String(localized: "The local file is not available."))
        }
        guard NSWorkspace.shared.open(url) else {
            throw RuntimeError.operation(String(localized: "macOS could not open this file. Choose a default application for it."))
        }
    }
}
