import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

struct AccountStoreTests {
    private let outbox = AccountOutbox()
    private let store: AccountStore

    init() {
        store = AccountStore(commands: outbox.commands, browser: "app")
        store.reset(userId: "user-1")
    }

    @Test func `insecure deletion page is not opened`() throws {
        store.deleteAccount()
        try store.receiveAuth(event("auth.deletion_pending", ["confirmationUri": "http://auth.kodosi.com/delete"]))
        #expect(store.deletion == .confirming(nil))
        #expect(store.deletionPage == nil)
    }
}

private final class AccountOutbox {
    let commands = CommandSink()

    init() {
        commands.transport { data in
            RuntimeContract.verify(data)
            return Int32(KODOSI_FFI_OK)
        }
    }
}

private func event(_ type: String, _ fields: [String: Any] = [:]) throws -> RuntimeEvent {
    var payload = fields
    payload["type"] = type
    payload["accountUserId"] = "user-1"
    payload["accountEpoch"] = 1
    return try JSONDecoder().decode(RuntimeEvent.self, from: JSONSerialization.data(withJSONObject: payload))
}
