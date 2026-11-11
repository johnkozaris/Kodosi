import Foundation

/// The app keeps these names in its app group for the notification extension.
nonisolated struct TerminalNames: Codable, Equatable {
    struct Entry: Codable, Equatable {
        let name: String
        let computer: String?
    }

    var terminals: [String: Entry] = [:]

    static let group = "group.com.kodosi.mobile"

    private static var file: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)?
            .appending(path: "terminal-names.json")
    }

    static func read() -> TerminalNames? {
        guard let file, let data = try? Data(contentsOf: file) else { return nil }
        return try? JSONDecoder().decode(TerminalNames.self, from: data)
    }

    func write() {
        guard let file = Self.file, let data = try? JSONEncoder().encode(self) else { return }
        try? data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    static func forget() {
        guard let file else { return }
        try? FileManager.default.removeItem(at: file)
    }
}
