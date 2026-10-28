import Foundation
import KodosiBridge

struct SessionFolderGroup: Identifiable {
    struct Owner: Equatable {
        let name: String
        let userId: String?
    }

    let id: String
    let directory: String?
    let computer: String?
    let computerId: String?
    let owner: Owner?
    let sessions: [RuntimeSession]

    var name: String {
        directory.map(HostPath.folderName) ?? String(localized: "Terminals")
    }

    var canStartTerminal: Bool {
        directory != nil && owner == nil && !away
    }

    var away: Bool {
        computer != nil && !sessions.isEmpty && sessions.allSatisfy(\.hostOffline)
    }
}

struct SessionComputerGroup: Identifiable {
    let id: String
    let computer: String?
    let owner: SessionFolderGroup.Owner?
    let title: String
    let folders: [SessionFolderGroup]

    var away: Bool {
        computer != nil && folders.allSatisfy(\.away)
    }

    private static func key(of session: RuntimeSession) -> String {
        session.kind == .local ? "local" : session.hostDeviceId ?? session.ownerUserId ?? session.id
    }

    static func groups(_ sessions: [RuntimeSession]) -> [Self] {
        let computers = Dictionary(grouping: sessions, by: key)
        let friendsComputers = Dictionary(grouping: computers.values.compactMap(\.first).filter { $0.kind != .local && !$0.isOwner }) {
            $0.ownerUserId ?? $0.ownerName ?? ""
        }
        return computers.map { key, sessions in
            let first = sessions[0]
            let remote = first.kind != .local
            let owner = remote && !first.isOwner
                ? SessionFolderGroup.Owner(name: first.ownerName ?? String(localized: "Shared"), userId: first.ownerUserId) : nil
            let computer = remote ? first.hostLabel : nil
            let title: String = if let owner {
                (friendsComputers[first.ownerUserId ?? first.ownerName ?? ""]?.count ?? 0) > 1
                    ? "\(owner.name) · \(computer ?? "")" : owner.name
            } else {
                computer ?? String(localized: "This Mac")
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
