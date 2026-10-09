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
            cursorBlink: false
        )

        func normalized() -> TerminalSettings {
            var copy = self
            let family = fontFamily.trimmingCharacters(in: .whitespacesAndNewlines)
            copy.fontFamily = family.isEmpty ? TerminalDefaults.fontFamily : family
            copy.fontSize = min(
                max(fontSize, TerminalDefaults.fontSizeRange.lowerBound),
                TerminalDefaults.fontSizeRange.upperBound
            )
            copy.lineHeight = min(
                max(lineHeight, TerminalDefaults.lineHeightRange.lowerBound),
                TerminalDefaults.lineHeightRange.upperBound
            )
            copy.scrollbackLines = min(
                max(scrollbackLines, TerminalDefaults.scrollbackRange.lowerBound),
                TerminalDefaults.scrollbackRange.upperBound
            )
            return copy
        }
    }

    var fontFamily: String = TerminalDefaults.fontFamily
    var cursorStyle: CursorStyle = .block
    var fontSize: Int = TerminalDefaults.fontSize
    var lineHeight: Double = TerminalDefaults.lineHeight
    var scrollbackLines: Int = TerminalDefaults.scrollbackLines
    var cursorBlink: Bool = false

    var lastWorkingDir: String?
    var startCommands: [StartCommand] = []
    private var learnedAgents: Set<String> = []

    enum CursorStyle: String, CaseIterable, Codable, Identifiable {
        case block, bar, underline
        var id: String {
            rawValue
        }

        var label: String {
            rawValue.capitalized
        }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        load()
    }

    var terminalSettings: TerminalSettings {
        TerminalSettings(
            fontFamily: fontFamily,
            cursorStyle: cursorStyle,
            fontSize: fontSize,
            lineHeight: lineHeight,
            scrollbackLines: scrollbackLines,
            cursorBlink: cursorBlink
        )
    }

    private func applyTerminalSettingsInMemory(_ settings: TerminalSettings) {
        fontFamily = settings.fontFamily
        cursorStyle = settings.cursorStyle
        fontSize = settings.fontSize
        lineHeight = settings.lineHeight
        scrollbackLines = settings.scrollbackLines
        cursorBlink = settings.cursorBlink
    }

    @discardableResult
    func commitTerminalSettings(_ candidate: TerminalSettings) -> TerminalSettings {
        let committed = candidate.normalized()
        applyTerminalSettingsInMemory(committed)
        persistTerminalSettings(committed)
        return committed
    }

    private func persistTerminalSettings(_ settings: TerminalSettings) {
        guard let data = try? JSONEncoder().encode(settings) else { return }
        defaults.set(data, forKey: Self.terminalSettingsKey)
    }

    func learn(_ agent: AgentKind) {
        guard agent.isAgent, learnedAgents.insert(agent.rawValue).inserted else { return }
        if !startCommands.contains(where: { $0.agent == agent }) {
            startCommands.append(StartCommand(name: agent.label, command: agent == .cursor ? "cursor-agent" : agent.rawValue))
        }
        saveStartCommands()
    }

    func saveStartCommands() {
        defaults.set(try? JSONEncoder().encode(startCommands), forKey: "agents.startCommands")
        defaults.set(learnedAgents.sorted(), forKey: "agents.learned")
    }

    func save() {
        _ = commitTerminalSettings(terminalSettings)
        let d = defaults
        if let dir = lastWorkingDir {
            d.set(dir, forKey: "session.lastWorkingDir")
        } else {
            d.removeObject(forKey: "session.lastWorkingDir")
        }
    }

    func load() {
        let d = defaults
        loadTerminalSettings(from: d)
        lastWorkingDir = d.string(forKey: "session.lastWorkingDir")
        startCommands = d.data(forKey: "agents.startCommands").flatMap { try? JSONDecoder().decode([StartCommand].self, from: $0) } ?? []
        learnedAgents = Set(d.stringArray(forKey: "agents.learned") ?? [])
        clampValues()
    }

    private func loadTerminalSettings(from defaults: UserDefaults) {
        let value = defaults.data(forKey: Self.terminalSettingsKey)
            .flatMap { try? JSONDecoder().decode(TerminalSettings.self, from: $0) }
            ?? .defaults
        applyTerminalSettingsInMemory(value.normalized())
    }

    func clampValues() {
        let normalized = terminalSettings.normalized()
        fontFamily = normalized.fontFamily
        cursorStyle = normalized.cursorStyle
        fontSize = normalized.fontSize
        lineHeight = normalized.lineHeight
        scrollbackLines = normalized.scrollbackLines
        cursorBlink = normalized.cursorBlink
    }
}
