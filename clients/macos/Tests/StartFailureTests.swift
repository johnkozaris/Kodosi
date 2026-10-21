import Foundation
import KodosiBridge
import KodosiKit
import Testing

@Test func `start failures map from the runtime contract`() {
    #expect(RuntimeStartFailure.Code(rawValue: KODOSI_START_HOST_OUTDATED) == .hostOutdated)
    #expect(RuntimeStartFailure.Code(rawValue: KODOSI_START_REJECTED) == .rejected)
    #expect(RuntimeStartFailure.Code(rawValue: 99) == .other(99))
    #expect(RuntimeStartFailure.HostKind(rawValue: KODOSI_HOST_KIND_BACKGROUND) == .background)
    #expect(RuntimeStartFailure.HostKind(rawValue: -5) == .unknown)
    var raw = kodosi_start_failure_t()
    raw.code = KODOSI_START_HOST_OUTDATED
    raw.host_kind = KODOSI_HOST_KIND_FOREGROUND
    raw.host_pid = 7
    raw.host_terminals = 1
    withUnsafeMutablePointer(to: &raw.message) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: Int(KODOSI_START_FAILURE_MESSAGE_BYTES)) { buffer in
            for (offset, byte) in "updated".utf8.enumerated() {
                buffer[offset] = CChar(bitPattern: byte)
            }
        }
    }
    #expect(
        RuntimeStartFailure(raw)
            == RuntimeStartFailure(code: .hostOutdated, hostKind: .foreground, hostPID: 7, hostTerminals: 1, message: "updated"),
    )
}
