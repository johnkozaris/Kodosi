import Foundation
import KodosiBridge
import Testing

struct RuntimeContractTests {
    @Test func `the schema lists the commands the app sends`() {
        let open = #"{"type":"session.openRemote","sessionId":"s","accountUserId":null,"accountEpoch":0}"#
        #expect(RuntimeContract.violation(in: Data(open.utf8)) != nil)
        let complete = #"{"type":"session.openRemote","sessionId":"s","requestId":"r","accountUserId":null,"accountEpoch":0}"#
        #expect(RuntimeContract.violation(in: Data(complete.utf8)) == nil)
    }

    @Test func `the app expects the schema version`() {
        #expect(RuntimeContract.version == Int(RuntimeHandle.expectedProtocolVersion))
    }
}
