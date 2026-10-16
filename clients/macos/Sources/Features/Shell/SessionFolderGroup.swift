import Foundation
import KodosiBridge

struct SessionFolderGroup: Identifiable {
    let id: String
    let directory: String?
    let host: String?
    let sessions: [RuntimeSession]

    var name: String {
        guard let directory else { return String(localized: "Terminals") }
        return HostPath.folderName(directory)
    }

    var label: String {
        guard let host else { return name }
        return directory == nil ? host : "\(name) · \(host)"
    }

    private static func computer(of session: RuntimeSession) -> String {
        session.kind == .local ? "local" : session.hostDeviceId ?? session.ownerUserId ?? session.id
    }

    private static func host(of session: RuntimeSession) -> String? {
        guard session.kind != .local else { return nil }
        return session.isOwner ? session.hostLabel : "\(session.ownerName ?? String(localized: "Shared")) · \(session.hostLabel)"
    }

    static func groups(_ sessions: [RuntimeSession]) -> [Self] {
        let grouped = Dictionary(grouping: sessions) { "\(computer(of: $0)):\($0.workingDir ?? "")" }
        return grouped.map { key, values in
            Self(id: key, directory: values[0].workingDir, host: host(of: values[0]), sessions: values)
        }.sorted {
            if ($0.host == nil) != ($1.host == nil) {
                return $0.host == nil
            }
            return ($0.host ?? "", $0.directory ?? "") < ($1.host ?? "", $1.directory ?? "")
        }
    }
}
