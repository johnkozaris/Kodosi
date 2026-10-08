import Foundation

struct SessionFolderGroup: Identifiable {
    let id: String
    let directory: String?
    let host: String?
    let sessions: [RuntimeSession]

    var name: String {
        guard let directory else { return String(localized: "Terminals") }
        return URL(fileURLWithPath: directory).lastPathComponent
    }

    static func groups(_ sessions: [RuntimeSession]) -> [Self] {
        let grouped = Dictionary(grouping: sessions) { session in
            "\(session.kind == .local ? "local" : session.hostDeviceId ?? session.ownerUserId ?? session.id):\(session.workingDir ?? "")"
        }
        return grouped.map { key, values in
            let session = values[0]
            let owner = session.ownerName ?? String(localized: "Shared")
            let host = session.kind == .local ? nil : session.isOwner ? session.hostLabel : "\(owner) · \(session.hostLabel)"
            return Self(id: key, directory: session.workingDir, host: host, sessions: values)
        }.sorted {
            if ($0.host == nil) != ($1.host == nil) {
                return $0.host == nil
            }
            return ($0.host ?? "", $0.directory ?? "") < ($1.host ?? "", $1.directory ?? "")
        }
    }
}
