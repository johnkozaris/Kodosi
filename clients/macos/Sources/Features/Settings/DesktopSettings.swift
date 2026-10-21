import Foundation
import Observation

@MainActor
@Observable
final class DesktopSettings {
    private let defaults: UserDefaults
    private static let terminalSettingsKey = "terminal.settings"

    enum TerminalDefaults {
        static let fontFamily = "JetBrains Mono"
        static let fontSize = 14
        static let fontSizeRange = 8 ... 32
        static let lineHeight = 1.1
        static let lineHeightRange = 0.8 ... 2.0
        static let scrollbackLines = 10000
        static let scrollbackRange = 100 ... 100_000
    }

    struct TerminalSettings: Codable, Equatable {
        var fontFamily: String
        var cursorStyle: CursorStyle
        var fontSize: Int
        var lineHeight: Double
        var scrollbackLines: Int
        var cursorBlink: Bool

        static let defaults = TerminalSettings(
            fontFamily: TerminalDefaults.fontFamily,
            cursorStyle: .block,
            fontSize: TerminalDefaults.fontSize,
            lineHeight: TerminalDefaults.lineHeight,
            scrollbackLines: TerminalDefaults.scrollbackLines,
            cursorBlink: false,
        )

        func normalized() -> TerminalSettings {
            var copy = self
            let family = fontFamily.trimmingCharacters(in: .whitespacesAndNewlines)
            copy.fontFamily = family.isEmpty ? TerminalDefaults.fontFamily : family
            copy.fontSize = min(
                max(fontSize, TerminalDefaults.fontSizeRange.lowerBound),
                TerminalDefaults.fontSizeRange.upperBound,
            )
            copy.lineHeight = min(
                max(lineHeight, TerminalDefaults.lineHeightRange.lowerBound),
                TerminalDefaults.lineHeightRange.upperBound,
            )
            copy.scrollbackLines = min(
                max(scrollbackLines, TerminalDefaults.scrollbackRange.lowerBound),
                TerminalDefaults.scrollbackRange.upperBound,
            )
            return copy
        }
    }

    private(set) var terminalSettings = TerminalSettings.defaults
    var lastWorkingDir: String?
    var branchFolder: String?

    enum CursorStyle: String, CaseIterable, Codable {
        case block, bar, underline

        var label: String {
            rawValue.capitalized
        }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        load()
    }

    func commitTerminalSettings(_ candidate: TerminalSettings) -> TerminalSettings {
        terminalSettings = candidate.normalized()
        defaults.set(try? JSONEncoder().encode(terminalSettings), forKey: Self.terminalSettingsKey)
        return terminalSettings
    }

    func save() {
        let d = defaults
        if let dir = lastWorkingDir {
            d.set(dir, forKey: "session.lastWorkingDir")
        } else {
            d.removeObject(forKey: "session.lastWorkingDir")
        }
        if let branchFolder {
            d.set(branchFolder, forKey: "session.branchFolder")
        } else {
            d.removeObject(forKey: "session.branchFolder")
        }
    }

    private func load() {
        let d = defaults
        terminalSettings = (d.data(forKey: Self.terminalSettingsKey)
            .flatMap { try? JSONDecoder().decode(TerminalSettings.self, from: $0) } ?? .defaults).normalized()
        lastWorkingDir = d.string(forKey: "session.lastWorkingDir")
        branchFolder = d.string(forKey: "session.branchFolder")
    }
}
