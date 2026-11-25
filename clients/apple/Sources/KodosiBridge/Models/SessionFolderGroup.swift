import Foundation

/// Terminals whose working directory is unknown (such as unopened shared terminals) are grouped
/// with `directory == nil`.
public struct SessionFolderGroup: Identifiable, Sendable {
    public struct Owner: Equatable, Sendable {
        public let name: String
        public let userId: String?

        public init(name: String, userId: String?) {
            self.name = name
            self.userId = userId
        }
    }

    public let id: String
    public let directory: String?
    public let computer: String?
    public let computerId: String?
    public let owner: Owner?
    public let sessions: [RuntimeSession]
    public let newestAgentTerminal: RuntimeSession?

    public var name: String {
        directory.map(HostPath.folderName) ?? String(localized: "Terminals", bundle: .module)
    }

    public var canStartTerminal: Bool {
        directory != nil && owner == nil && !away
    }

    public var away: Bool {
        computer != nil && !sessions.isEmpty && sessions.allSatisfy(\.hostOffline)
    }
}

public struct SessionComputerGroup: Identifiable, Sendable {
    public let id: String
    public let computer: String?
    public let owner: SessionFolderGroup.Owner?
    public let title: String
    public let folders: [SessionFolderGroup]

    public var away: Bool {
        computer != nil && folders.allSatisfy(\.away)
    }

    public var computerId: String? {
        folders.first?.computerId
    }

    public var sessions: [RuntimeSession] {
        folders.flatMap(\.sessions)
    }

    private static func key(of session: RuntimeSession) -> String {
        session.kind == .local ? "local" : session.hostDeviceId ?? session.ownerUserId ?? session.id
    }

    /// Pinned terminals are omitted from each folder's `sessions` list but still count toward its
    /// `newestAgentTerminal`. Session IDs are UUIDv7, so lexicographic max yields the newest one.
    public static func groups(_ sessions: [RuntimeSession], pinned: Set<String> = []) -> [Self] {
        let computers = Dictionary(grouping: sessions, by: key)
        let friendsComputers = Dictionary(grouping: computers.values.compactMap(\.first).filter { $0.kind != .local && !$0.isOwner }) {
            $0.ownerUserId ?? $0.ownerName ?? ""
        }
        return computers.compactMap { key, sessions in
            let first = sessions[0]
            let remote = first.kind != .local
            let owner = remote && !first.isOwner
                ? SessionFolderGroup.Owner(name: first.ownerName ?? String(localized: "Shared", bundle: .module), userId: first.ownerUserId)
                : nil
            let computer = remote ? first.hostLabel : nil
            let title: String = if let owner {
                (friendsComputers[first.ownerUserId ?? first.ownerName ?? ""]?.count ?? 0) > 1
                    ? "\(owner.name) · \(computer ?? "")" : owner.name
            } else {
                computer ?? String(localized: "This Mac", bundle: .module)
            }
            let folders = Dictionary(grouping: sessions) { $0.workingDir ?? "" }.compactMap { directory, sessions in
                let shown = sessions.filter { !pinned.contains($0.id) }
                return shown.isEmpty ? nil : SessionFolderGroup(
                    id: "\(key):\(directory)", directory: directory.isEmpty ? nil : directory, computer: computer,
                    computerId: remote ? first.hostDeviceId : nil, owner: owner, sessions: shown,
                    newestAgentTerminal: sessions.filter { $0.program != nil }.max { $0.id < $1.id },
                )
            }.sorted { ($0.directory == nil ? 1 : 0, $0.directory ?? "") < ($1.directory == nil ? 1 : 0, $1.directory ?? "") }
            return folders.isEmpty ? nil : Self(id: key, computer: computer, owner: owner, title: title, folders: folders)
        }.sorted { first, second in
            guard first.rank == second.rank else { return first.rank < second.rank }
            let order = first.title.localizedCaseInsensitiveCompare(second.title)
            return order == .orderedSame ? first.id < second.id : order == .orderedAscending
        }
    }

    private var rank: Int {
        computer == nil ? 0 : owner == nil ? 1 : 2
    }
}
