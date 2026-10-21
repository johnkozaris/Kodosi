import Foundation

public struct DebugBackendConfiguration: Equatable, Sendable {
    public static let hostInfoKey = "KodosiDebugBackendHost"
    public static let portInfoKey = "KodosiDebugBackendPort"

    public let api: String

    public init(api: String) {
        self.api = api
    }

    public static func resolve(info: [String: Any]) -> DebugBackendConfiguration? {
        guard let rawHost = info[hostInfoKey] as? String,
              let rawPort = info[portInfoKey] as? String
        else { return nil }
        let host = rawHost.trimmingCharacters(in: .whitespacesAndNewlines)
        guard host == "127.0.0.1" || host == "localhost",
              let port = UInt16(rawPort),
              port > 0
        else { return nil }
        return DebugBackendConfiguration(
            api: "http://\(host):\(port)",
        )
    }

    public func install() throws {
        let key = "KODOSI__BACKEND__API"
        guard setenv(key, api, 1) == 0 else {
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
