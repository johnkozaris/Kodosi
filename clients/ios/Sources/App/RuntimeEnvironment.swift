import Foundation
import KodosiBridge
import UIKit

enum RuntimeEnvironment {
    static func install() throws {
        let fileManager = FileManager.default
        let support = try fileManager.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true,
        )
        var root = support.appending(path: "kodosi", directoryHint: .isDirectory)
        try fileManager.createDirectory(at: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try root.setResourceValues(values)
        try set("KODOSI_PRODUCTION_DATA_ROOT", root.path)
        try set("KODOSI_DEVICE_LABEL", UIDevice.current.name)
        #if DEBUG
            try DebugBackendConfiguration.resolve(info: Bundle.main.infoDictionary ?? [:])?.install()
        #endif
    }

    private static func set(_ key: String, _ value: String) throws {
        guard setenv(key, value, 1) == 0 else {
            throw RuntimeError.operation(String(localized: "Kodosi could not prepare its storage."))
        }
    }
}
