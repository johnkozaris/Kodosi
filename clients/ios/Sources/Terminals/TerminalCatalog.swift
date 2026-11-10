import Foundation
import KodosiBridge
import KodosiDesign

enum TerminalCatalog {
    static func needsYou(_ sessions: [RuntimeSession]) -> [RuntimeSession] {
        sessions.filter { $0.needsUser && $0.canCall }
    }

    /// A new terminal on the computer, when it is one of yours that is online.
    static func newTerminal(on computer: SessionComputerGroup, online: [String]) -> NewTerminalRequest? {
        guard computer.owner == nil, let id = computer.computerId, online.contains(id) else { return nil }
        return NewTerminalRequest(computer: id)
    }

    /// A new terminal in the folder, when it is on one of your computers that is online.
    static func newTerminal(in folder: SessionFolderGroup, online: [String]) -> NewTerminalRequest? {
        guard folder.canStartTerminal, let id = folder.computerId, online.contains(id) else { return nil }
        return NewTerminalRequest(computer: id, folder: folder.directory)
    }

    static func matching(_ sessions: [RuntimeSession], query: String) -> [RuntimeSession] {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return sessions }
        return sessions.filter { session in
            [session.name, session.activity, session.hostLabel, session.ownerName, session.missionName, session.folderName, session.agent.label]
                .compactMap(\.self)
                .contains { $0.localizedStandardContains(query) }
        }
    }
}
