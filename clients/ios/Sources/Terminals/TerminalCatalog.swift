import Foundation
import KodosiBridge
import KodosiDesign

struct TerminalGroup: Identifiable, Equatable {
    enum Place: Hashable {
        case computer(id: String, name: String)
        case friend(id: String, name: String)
    }

    let place: Place
    let sessions: [RuntimeSession]

    var id: Place {
        place
    }

    var title: String {
        switch place {
        case let .computer(_, name), let .friend(_, name): name
        }
    }

    /// The friend whose terminals these are, by their identifier.
    var friend: String? {
        guard case let .friend(id, _) = place else { return nil }
        return id
    }

    /// Whether the computers of these terminals are offline.
    var away: Bool {
        !sessions.isEmpty && sessions.allSatisfy(\.hostOffline)
    }
}

enum TerminalCatalog {
    static func needsYou(_ sessions: [RuntimeSession]) -> [RuntimeSession] {
        sessions.filter { $0.needsUser && $0.canCall }
    }

    /// Your terminals by computer, then the terminals of each friend.
    static func groups(_ sessions: [RuntimeSession]) -> [TerminalGroup] {
        var computers: [String: (name: String, sessions: [RuntimeSession])] = [:]
        var friends: [String: (name: String, sessions: [RuntimeSession])] = [:]
        for session in sessions {
            if session.isOwner {
                computers[session.hostDeviceId ?? session.hostLabel, default: (session.hostLabel, [])].sessions.append(session)
            } else {
                let name = session.ownerName ?? String(localized: "Shared with you")
                friends[session.ownerUserId ?? name, default: (name, [])].sessions.append(session)
            }
        }
        return sorted(computers).map { TerminalGroup(place: .computer(id: $0.key, name: $0.value.name), sessions: $0.value.sessions) }
            + sorted(friends).map { TerminalGroup(place: .friend(id: $0.key, name: $0.value.name), sessions: $0.value.sessions) }
    }

    private static func sorted(
        _ groups: [String: (name: String, sessions: [RuntimeSession])],
    ) -> [(key: String, value: (name: String, sessions: [RuntimeSession]))] {
        groups.sorted { lhs, rhs in
            let order = lhs.value.name.localizedStandardCompare(rhs.value.name)
            return order == .orderedSame ? lhs.key < rhs.key : order == .orderedAscending
        }
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
