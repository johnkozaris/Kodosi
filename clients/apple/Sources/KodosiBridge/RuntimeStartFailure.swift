import Foundation
import KodosiKit

public struct RuntimeStartFailure: Equatable, Sendable {
    public enum Code: Equatable, Sendable {
        case hostOutdated, other
    }

    public enum HostKind: Equatable, Sendable {
        case unknown, foreground, background

        public init(rawValue: Int32) {
            switch rawValue {
            case KODOSI_HOST_KIND_FOREGROUND: self = .foreground
            case KODOSI_HOST_KIND_BACKGROUND: self = .background
            default: self = .unknown
            }
        }
    }

    public let code: Code
    public let hostKind: HostKind
    public let hostPID: UInt32
    public let hostTerminals: UInt32
    public let message: String

    public var restartsWithoutAsking: Bool {
        code == .hostOutdated && hostKind == .background && hostTerminals == 0
    }

    public init(code: Code, hostKind: HostKind, hostPID: UInt32, hostTerminals: UInt32, message: String) {
        self.code = code
        self.hostKind = hostKind
        self.hostPID = hostPID
        self.hostTerminals = hostTerminals
        self.message = message
    }

    public init(_ raw: kodosi_start_failure_t) {
        var raw = raw
        let message = withUnsafePointer(to: &raw.message) { pointer in
            pointer.withMemoryRebound(to: CChar.self, capacity: Int(KODOSI_START_FAILURE_MESSAGE_BYTES)) {
                String(cString: $0)
            }
        }
        self.init(
            code: raw.code == KODOSI_START_HOST_OUTDATED ? .hostOutdated : .other, hostKind: HostKind(rawValue: raw.host_kind),
            hostPID: raw.host_pid, hostTerminals: raw.host_terminals, message: message,
        )
    }

    public static func last() -> RuntimeStartFailure? {
        var raw = kodosi_start_failure_t()
        guard kodosi_last_start_failure(&raw) == 1 else { return nil }
        return RuntimeStartFailure(raw)
    }
}

public enum HostStopResult: Equatable, Sendable {
    case accepted, refused, unreachable, failed

    /// `update` tells the host that it stops to install an update and comes right back.
    @concurrent
    public static func stopHost(force: Bool, update: Bool = false) async -> HostStopResult {
        switch kodosi_host_stop(force ? 1 : 0, update ? 1 : 0) {
        case KODOSI_HOST_STOP_ACCEPTED: .accepted
        case KODOSI_HOST_STOP_REFUSED: .refused
        case KODOSI_HOST_STOP_UNREACHABLE: .unreachable
        default: .failed
        }
    }
}
