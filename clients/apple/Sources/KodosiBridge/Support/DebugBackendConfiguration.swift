import Foundation

public struct DebugBackendConfiguration: Equatable, Sendable {
    public static let hostInfoKey = "KodosiDebugBackendHost"
    public static let portInfoKey = "KodosiDebugBackendPort"
    public static let issuerInfoKey = "KodosiDebugAuthIssuer"
    private static let loopbackHosts: Set<String> = ["127.0.0.1", "localhost"]

    public let api: String
    public let issuer: String?

    public init(api: String, issuer: String? = nil) {
        self.api = api
        self.issuer = issuer
    }

    public static func resolve(info: [String: Any]) -> DebugBackendConfiguration? {
        guard let rawHost = info[hostInfoKey] as? String,
              let rawPort = info[portInfoKey] as? String
        else { return nil }
        let host = rawHost.trimmingCharacters(in: .whitespacesAndNewlines)
        guard loopbackHosts.contains(host),
              let port = UInt16(rawPort),
              port > 0
        else { return nil }
        return DebugBackendConfiguration(
            api: "http://\(host):\(port)",
            issuer: loopbackIssuer(info[issuerInfoKey] as? String),
        )
    }

    private static func loopbackIssuer(_ raw: String?) -> String? {
        guard let raw, let issuer = URL(string: raw.trimmingCharacters(in: .whitespacesAndNewlines)),
              issuer.scheme == "http", let host = issuer.host(), loopbackHosts.contains(host)
        else { return nil }
        return issuer.absoluteString
    }

    /// Preserves an existing `KODOSI__AUTH__ISSUER` from the launch environment so a single build
    /// can target a different local auth service.
    public func install() throws {
        try set("KODOSI__BACKEND__API", api, replacing: true)
        if let issuer {
            try set("KODOSI__AUTH__ISSUER", issuer, replacing: false)
        }
    }

    private func set(_ key: String, _ value: String, replacing: Bool) throws {
        guard setenv(key, value, replacing ? 1 : 0) == 0 else {
            throw InstallationError.environmentUpdateFailed(key)
        }
    }

    public enum InstallationError: LocalizedError, Equatable {
        case environmentUpdateFailed(String)

        public var errorDescription: String? {
            switch self {
            case let .environmentUpdateFailed(key):
                String(localized: "Could not install Debug backend setting \(key).", bundle: .module)
            }
        }
    }
}
