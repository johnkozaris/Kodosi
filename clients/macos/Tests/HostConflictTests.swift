import Foundation
@testable import KodosiDesktop
import KodosiKit
import Testing

private func failure(
    _ kind: RuntimeStartFailure.HostKind, sessions: UInt32, code: RuntimeStartFailure.Code = .hostBusy
) -> RuntimeStartFailure {
    RuntimeStartFailure(code: code, hostKind: kind, hostPID: 42, hostLocalSessions: sessions, message: "busy")
}

@Test func idleBackgroundHostsAreTakenOverAndOthersAskFirst() {
    #expect(failure(.background, sessions: 0).resolution == .takeOverIdleHost)
    #expect(failure(.background, sessions: 2).resolution == .askToStopHost)
    #expect(failure(.foreground, sessions: 0).resolution == .askToStopHost)
    #expect(failure(.unknown, sessions: 0).resolution == .askToStopHost)
    #expect(failure(.app, sessions: 0).resolution == .quitDuplicateApp)
    #expect(failure(.background, sessions: 0, code: .rejected).resolution == .report)
    #expect(failure(.background, sessions: 0, code: .failed).resolution == .report)
}

@Test func startFailuresMapFromTheRuntimeContract() {
    #expect(RuntimeStartFailure.Code(rawValue: KODOSI_START_HOST_BUSY) == .hostBusy)
    #expect(RuntimeStartFailure.Code(rawValue: KODOSI_START_REJECTED) == .rejected)
    #expect(RuntimeStartFailure.Code(rawValue: 99) == .other(99))
    #expect(RuntimeStartFailure.HostKind(rawValue: KODOSI_HOST_KIND_BACKGROUND) == .background)
    #expect(RuntimeStartFailure.HostKind(rawValue: -5) == .unknown)
    var raw = kodosi_start_failure_t()
    raw.code = KODOSI_START_HOST_BUSY
    raw.host_kind = KODOSI_HOST_KIND_FOREGROUND
    raw.host_pid = 7
    raw.host_local_sessions = 1
    withUnsafeMutablePointer(to: &raw.message) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: Int(KODOSI_START_FAILURE_MESSAGE_BYTES)) { buffer in
            for (offset, byte) in "lock held".utf8.enumerated() {
                buffer[offset] = CChar(bitPattern: byte)
            }
        }
    }
    #expect(
        RuntimeStartFailure(raw)
            == RuntimeStartFailure(code: .hostBusy, hostKind: .foreground, hostPID: 7, hostLocalSessions: 1, message: "lock held")
    )
}
