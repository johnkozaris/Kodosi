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
    let owner: Owner?
    let sessions: [RuntimeSession]

    var name: String {
        if let directory {
            return HostPath.folderName(directory)
        }
        return owner?.name ?? String(localized: "Terminals")
    }

    private static func key(of session: RuntimeSession) -> String {
        session.kind == .local ? "local" : session.hostDeviceId ?? session.ownerUserId ?? session.id
    }

    static func groups(_ sessions: [RuntimeSession]) -> [Self] {
        let grouped = Dictionary(grouping: sessions) { "\(key(of: $0)):\($0.workingDir ?? "")" }
        return grouped.map { key, values in
            let first = values[0]
            let remote = first.kind != .local
            return Self(
                id: key, directory: first.workingDir, computer: remote ? first.hostLabel : nil,
                owner: remote && !first.isOwner
                    ? Owner(name: first.ownerName ?? String(localized: "Shared"), userId: first.ownerUserId) : nil,
                sessions: values,
            )
        }.sorted {
            if ($0.computer == nil) != ($1.computer == nil) {
                return $0.computer == nil
            }
            return ($0.owner?.name ?? "", $0.computer ?? "", $0.directory ?? "") < ($1.owner?.name ?? "", $1.computer ?? "", $1.directory ?? "")
        }
    }
}
