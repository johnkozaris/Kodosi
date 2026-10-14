import Foundation
import KodosiBridge
import KodosiDesign

struct TerminalGroup: Identifiable, Equatable {
    enum Place: Hashable {
        case computer(id: String, name: String)
        case shared
    }

    let place: Place
    let sessions: [RuntimeSession]

    var id: Place {
        place
    }

    var title: String {
        switch place {
        case let .computer(_, name): name
        case .shared: String(localized: "Shared with you")
        }
    }

    var symbol: String {
        switch place {
        case let .computer(_, name): DeviceGlyph.symbol(for: name)
        case .shared: "person.2.fill"
        }
    }
}

enum TerminalCatalog {
    static func needsYou(_ sessions: [RuntimeSession]) -> [RuntimeSession] {
        sessions.filter(waitsForYou)
    }

    static func groups(_ sessions: [RuntimeSession]) -> [TerminalGroup] {
        var computers: [String: (name: String, sessions: [RuntimeSession])] = [:]
        var shared: [RuntimeSession] = []
        for session in sessions where !waitsForYou(session) {
            guard session.isOwner else {
                shared.append(session)
                continue
            }
            let id = session.hostDeviceId ?? session.hostLabel
            computers[id, default: (session.hostLabel, [])].sessions.append(session)
        }
        var groups = computers
            .sorted { lhs, rhs in
                let order = lhs.value.name.localizedStandardCompare(rhs.value.name)
                return order == .orderedSame ? lhs.key < rhs.key : order == .orderedAscending
            }
            .map { TerminalGroup(place: .computer(id: $0.key, name: $0.value.name), sessions: $0.value.sessions) }
        if !shared.isEmpty {
            groups.append(TerminalGroup(place: .shared, sessions: shared))
        }
        return groups
    }

    private static func waitsForYou(_ session: RuntimeSession) -> Bool {
        session.needsUser && session.canCall
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
