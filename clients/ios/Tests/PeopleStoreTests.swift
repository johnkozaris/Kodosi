import Foundation
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct PeopleStoreTests {
    private let outbox = PeopleOutbox()
    private let store: PeopleStore

    init() {
        store = PeopleStore(commands: outbox.commands)
        store.reset(userId: "user-1")
    }

    @Test func `snapshot replaces friends and requests`() throws {
        #expect(!store.loaded)
        try deliverSnapshot()
        #expect(store.loaded)
        #expect(store.friends.map(\.handle) == ["ana", "bo"])
        #expect(store.friends[0].isVerified)
        #expect(store.friends[1].identityChanged)
        #expect(!store.friends[1].isVerified)
        #expect(store.incoming.map(\.handle) == ["cy"])
        #expect(store.outgoing.map(\.handle) == ["di"])
        #expect(store.blocked.map(\.handle) == ["ed"])
        #expect(store.name(of: "id-ana") == "Ana")
        #expect(store.name(of: "id-bo") == "bo")
        #expect(store.revision == 1)
    }

    @Test func `request actions use the handle and settle on the next snapshot`() throws {
        try deliverSnapshot()
        let incoming = try #require(store.incoming.first)
        let outgoing = try #require(store.outgoing.first)
        store.accept(incoming)
        store.accept(incoming)
        store.cancel(outgoing)
        #expect(outbox.types == ["friends.request.accept", "friends.request.cancel"])
        #expect(outbox.sent.map { $0["username"] } == [.string("cy"), .string("di")])
        #expect(store.isBusy("CY"))
        #expect(store.isBusy("di"))
        try deliverSnapshot()
        #expect(store.inFlight.isEmpty)
        store.decline(incoming)
        var declined = try #require(outbox.sent.last)
        #expect(declined.removeValue(forKey: "requestId") != nil)
        #expect(declined == ["type": .string("friends.request.reject"), "username": .string("cy"),
                             "accountUserId": .null, "accountEpoch": .int(0)])
    }

    @Test func `invite request keeps the invite text`() throws {
        store.requestInvite()
        store.requestInvite()
        #expect(outbox.types == ["friends.invite"])
        #expect(store.fetchingInvite)
        try store.receive(event("friends.result", ["operation": "friends.invite", "requestId": "request-1", "text": "kodosi:me:0a1b"]))
        #expect(store.ownInvite == "kodosi:me:0a1b")
        #expect(!store.fetchingInvite)
    }

    @Test func `only friend errors settle pending work`() throws {
        try deliverSnapshot()
        let ana = try #require(store.friend("id-ana"))
        store.remove(ana)
        try store.absorb(error: event("devices.error", ["operation": "devices.revoke", "message": "No"]))
        #expect(store.isBusy("ana"))
        try store.absorb(error: event("friends.error", ["operation": "friends.remove", "message": "No"]))
        #expect(!store.isBusy("ana"))
    }

    @Test func `reset forgets the earlier account`() throws {
        try deliverSnapshot()
        try store.receive(event("friends.result", ["operation": "friends.invite", "requestId": "request-1", "text": "kodosi:me:0a1b"]))
        store.reset(userId: "user-2")
        #expect(store.userId == "user-2")
        #expect(store.isEmpty)
        #expect(!store.loaded)
        #expect(store.ownInvite == nil)
    }

    @Test func `refresh waits for the next snapshot`() async throws {
        let refresh = Task { await store.refresh() }
        for _ in 0 ..< 100 where outbox.sent.isEmpty {
            await Task.yield()
        }
        #expect(outbox.types == ["friends.refresh"])
        try deliverSnapshot()
        await refresh.value
        #expect(store.loaded)
    }

    private func deliverSnapshot() throws {
        try store.receive(event("friends.snapshot", [
            "friends": [
                ["userId": "id-ana", "handle": "ana", "displayName": "Ana", "verified": true, "identityState": "fixed", "inviteNeeded": false],
                ["userId": "id-bo", "handle": "bo", "displayName": "", "verified": true, "identityState": "changed", "inviteNeeded": false],
            ],
            "incoming": [["userId": "id-cy", "handle": "cy", "displayName": "Cy", "createdAt": "2026-10-11T10:00:00Z"]],
            "outgoing": [["userId": "id-di", "handle": "di", "displayName": "Di", "createdAt": "2026-10-11T11:00:00Z"]],
            "blocked": [["userId": "id-ed", "handle": "ed", "displayName": "Ed"]],
        ]))
    }
}

@MainActor
private final class PeopleOutbox {
    let commands = CommandSink()
    private(set) var sent: [[String: JSONValue]] = []

    init() {
        commands.transport = { [weak self] data in
            RuntimeContract.verify(data)
            if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
                self?.sent.append(command)
            }
            return Int32(KODOSI_FFI_OK)
        }
    }

    var types: [String] {
        sent.compactMap { $0["type"]?.stringValue }
    }
}

private func event(_ type: String, _ fields: [String: Any] = [:]) throws -> RuntimeEvent {
    var payload = fields
    payload["type"] = type
    payload["accountUserId"] = "user-1"
    payload["accountEpoch"] = 1
    return try JSONDecoder().decode(RuntimeEvent.self, from: JSONSerialization.data(withJSONObject: payload))
}
