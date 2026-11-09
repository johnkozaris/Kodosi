import Foundation

/// The terminals of one folder of a computer. A terminal whose folder is not known, such as a
/// friend's terminal that you have not opened, is in the group without a directory.
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

/// The terminals of one computer in their folders: this device's, another of yours, or a
/// friend's.
public struct SessionComputerGroup: Identifiable, Sendable {
    public let id: String
    public let computer: String?
    public let owner: SessionFolderGroup.Owner?
    public let title: String
    public let folders: [SessionFolderGroup]

    public var away: Bool {
        computer != nil && folders.allSatisfy(\.away)
    }

    /// The computer's device, when it is not this device.
    public var computerId: String? {
        folders.first?.computerId
    }

    public var sessions: [RuntimeSession] {
        folders.flatMap(\.sessions)
    }

    private static func key(of session: RuntimeSession) -> String {
        session.kind == .local ? "local" : session.hostDeviceId ?? session.ownerUserId ?? session.id
    }

    /// This device's terminals, then those of your other computers, then those of each friend's
    /// computer, each in its folders and then the terminals without a known folder.
    public static func groups(_ sessions: [RuntimeSession]) -> [Self] {
        let computers = Dictionary(grouping: sessions, by: key)
        let friendsComputers = Dictionary(grouping: computers.values.compactMap(\.first).filter { $0.kind != .local && !$0.isOwner }) {
            $0.ownerUserId ?? $0.ownerName ?? ""
        }
        return computers.map { key, sessions in
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
            let folders = Dictionary(grouping: sessions) { $0.workingDir ?? "" }.map { directory, sessions in
                SessionFolderGroup(
                    id: "\(key):\(directory)", directory: directory.isEmpty ? nil : directory, computer: computer,
                    computerId: remote ? first.hostDeviceId : nil, owner: owner, sessions: sessions,
                )
            }.sorted { ($0.directory == nil ? 1 : 0, $0.directory ?? "") < ($1.directory == nil ? 1 : 0, $1.directory ?? "") }
            return Self(id: key, computer: computer, owner: owner, title: title, folders: folders)
        }.sorted { ($0.rank, $0.title) < ($1.rank, $1.title) }
    }

    private var rank: Int {
        computer == nil ? 0 : owner == nil ? 1 : 2
    }
}
