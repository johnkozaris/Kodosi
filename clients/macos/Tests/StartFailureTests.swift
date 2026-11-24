import KodosiBridge
import Testing

@Test func `only an earlier background host with no terminals restarts without asking`() {
    func failure(_ kind: RuntimeStartFailure.HostKind, terminals: UInt32) -> RuntimeStartFailure {
        RuntimeStartFailure(code: .hostOutdated, hostKind: kind, hostPID: 7, hostTerminals: terminals, message: "")
    }
    #expect(failure(.background, terminals: 0).restartsWithoutAsking)
    #expect(!failure(.background, terminals: 1).restartsWithoutAsking)
    #expect(!failure(.foreground, terminals: 0).restartsWithoutAsking)
    #expect(!RuntimeStartFailure(code: .other, hostKind: .background, hostPID: 7, hostTerminals: 0, message: "")
        .restartsWithoutAsking)
}
