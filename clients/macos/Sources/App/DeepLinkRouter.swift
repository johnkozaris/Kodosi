import Foundation

struct DeepLinkDestination: Equatable {
    let sessionId: String
}

enum DeepLinkRouter {
    static func destination(for url: URL) -> DeepLinkDestination? {
        guard url.scheme?.lowercased() == "kodosi", url.host == "session",
              url.user == nil, url.password == nil, url.port == nil,
              url.query == nil, url.fragment == nil
        else { return nil }
        let parts = url.pathComponents.filter { $0 != "/" }
        guard parts.count == 1, let id = UUID(uuidString: parts[0]),
              id.uuidString.lowercased() == parts[0].lowercased()
        else { return nil }
        return DeepLinkDestination(sessionId: id.uuidString.lowercased())
    }
}
