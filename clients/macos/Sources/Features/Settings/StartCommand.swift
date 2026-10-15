import Foundation
import KodosiDesign

struct StartCommand: Codable, Equatable, Identifiable {
    var id = UUID().uuidString
    var name: String
    var command: String

    var agent: AgentKind {
        let program = command.split(whereSeparator: \.isWhitespace).first { $0 != "env" && !$0.contains("=") }
        return AgentKind(program: program.map { URL(fileURLWithPath: String($0)).lastPathComponent })
    }

    var line: String? {
        let line = command.trimmingCharacters(in: .whitespacesAndNewlines)
        return line.isEmpty || line.utf8.count > 1024 || line.unicodeScalars.contains(where: \.properties.generalCategory.isControl) ? nil : line
    }
}

private extension Unicode.GeneralCategory {
    var isControl: Bool {
        self == .control || self == .format || self == .lineSeparator || self == .paragraphSeparator
    }
}
