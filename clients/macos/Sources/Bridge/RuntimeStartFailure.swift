import Foundation
import KodosiKit

struct RuntimeStartFailure: Equatable, Sendable {
    enum Code: Equatable, Sendable {
        case invalidCallbacks, alreadyActive, hostBusy, rejected, failed
        case other(Int32)

        init(rawValue: Int32) {
            switch rawValue {
            case KODOSI_START_INVALID_CALLBACKS: self = .invalidCallbacks
            case KODOSI_START_ALREADY_ACTIVE: self = .alreadyActive
            case KODOSI_START_HOST_BUSY: self = .hostBusy
            case KODOSI_START_REJECTED: self = .rejected
            case KODOSI_START_FAILED: self = .failed
            default: self = .other(rawValue)
            }
        }
    }

    enum HostKind: Equatable, Sendable {
        case unknown, app, foreground, background

        init(rawValue: Int32) {
            switch rawValue {
            case KODOSI_HOST_KIND_APP: self = .app
            case KODOSI_HOST_KIND_FOREGROUND: self = .foreground
            case KODOSI_HOST_KIND_BACKGROUND: self = .background
            default: self = .unknown
            }
        }
    }

    enum Resolution: Equatable, Sendable {
        case takeOverIdleHost, askToStopHost, quitDuplicateApp, report
    }

    let code: Code
    let hostKind: HostKind
    let hostPID: UInt32
    let hostLocalSessions: UInt32
    let message: String

    init(code: Code, hostKind: HostKind, hostPID: UInt32, hostLocalSessions: UInt32, message: String) {
        self.code = code
        self.hostKind = hostKind
        self.hostPID = hostPID
        self.hostLocalSessions = hostLocalSessions
        self.message = message
    }

    init(_ raw: kodosi_start_failure_t) {
        var raw = raw
        let message = withUnsafePointer(to: &raw.message) { pointer in
            pointer.withMemoryRebound(to: CChar.self, capacity: Int(KODOSI_START_FAILURE_MESSAGE_BYTES)) {
                String(cString: $0)
            }
        }
        self.init(
            code: Code(rawValue: raw.code), hostKind: HostKind(rawValue: raw.host_kind),
            hostPID: raw.host_pid, hostLocalSessions: raw.host_local_sessions, message: message
        )
    }

    var resolution: Resolution {
        guard code == .hostBusy else { return .report }
        switch hostKind {
        case .app: return .quitDuplicateApp
        case .background where hostLocalSessions == 0: return .takeOverIdleHost
        case .background, .foreground, .unknown: return .askToStopHost
        }
    }

    static func last() -> RuntimeStartFailure? {
        var raw = kodosi_start_failure_t()
        guard kodosi_last_start_failure(&raw) == 1 else { return nil }
        return RuntimeStartFailure(raw)
    }
}

enum HostStopResult: Equatable, Sendable {
    case accepted, refused, unreachable, failed

    static func stopOtherHost(force: Bool) async -> HostStopResult {
        await Task.detached(priority: .userInitiated) {
            switch kodosi_host_stop(force ? 1 : 0) {
            case KODOSI_HOST_STOP_ACCEPTED: HostStopResult.accepted
            case KODOSI_HOST_STOP_REFUSED: .refused
            case KODOSI_HOST_STOP_UNREACHABLE: .unreachable
            default: .failed
            }
        }.value
    }
}
