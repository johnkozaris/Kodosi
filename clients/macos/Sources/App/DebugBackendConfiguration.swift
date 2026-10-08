import Foundation

struct DebugBackendConfiguration: Equatable {
    static let hostInfoKey = "KodosiDebugBackendHost"
    static let portInfoKey = "KodosiDebugBackendPort"

    let api: String

    static func resolve(info: [String: Any]) -> DebugBackendConfiguration? {
        guard let rawHost = info[hostInfoKey] as? String,
              let rawPort = info[portInfoKey] as? String
        else { return nil }
        let host = rawHost.trimmingCharacters(in: .whitespacesAndNewlines)
        guard host == "127.0.0.1" || host == "localhost",
              let port = UInt16(rawPort),
              port > 0
        else { return nil }
        return DebugBackendConfiguration(
            api: "http://\(host):\(port)"
        )
    }

    func install() throws {
        let key = "KODOSI__BACKEND__API"
        guard setenv(key, api, 1) == 0 else {
            throw InstallationError.environmentUpdateFailed(key)
        }
    }

    enum InstallationError: LocalizedError, Equatable {
        case environmentUpdateFailed(String)

        var errorDescription: String? {
            switch self {
            case let .environmentUpdateFailed(key):
                String(localized: "Could not install Debug backend setting \(key).")
            }
        }
    }
}
