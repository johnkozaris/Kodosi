import Foundation

enum ExternalAuthURL {
    static func parse(_ raw: String) -> URL? {
        guard let components = URLComponents(string: raw),
              components.scheme?.lowercased() == "https" || isLoopback(components),
              components.host?.isEmpty == false,
              components.user == nil,
              components.password == nil,
              let url = components.url
        else {
            return nil
        }
        return url
    }

    private static func isLoopback(_ components: URLComponents) -> Bool {
        components.scheme?.lowercased() == "http" && ["127.0.0.1", "localhost", "::1", "[::1]"].contains(components.host ?? "")
    }
}
