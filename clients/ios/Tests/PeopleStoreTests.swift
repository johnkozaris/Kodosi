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

    @Test func snapshotReplacesFriendsAndRequests() throws {
        #expect(!store.loaded)
        try deliverSnapshot()
        #expect(store.loaded)
        #expect(store.friends.map(\.handle) == ["ana", "bo"])
        #expect(store.friends[0].isVerified)
        #expect(store.friends[1].identityChanged)
        #expect(!store.friends[1].isVerified)
        #expect(store.incoming.map(\.handle) == ["cy"])
        #expect(store.outgoing.map(\.handle) == ["di"])
        #expect(store.name(of: "id-ana") == "Ana")
        #expect(store.name(of: "id-bo") == "bo")
        #expect(store.revision == 1)
    }

    @Test func addingSendsTheTrimmedTextWithARequestId() throws {
        #expect(!store.add("   "))
        #expect(outbox.sent.isEmpty)
        #expect(store.add("  kodosi:ana:00ff  "))
        let command = try #require(outbox.sent.last)
        #expect(command["type"] == .string("friends.request.send"))
        #expect(command["username"] == .string("kodosi:ana:00ff"))
        let requestId = try #require(command["requestId"]?.stringValue)
        #expect(UUID(uuidString: requestId) != nil)
        #expect(store.sendingRequest)
        try deliverSnapshot()
        #expect(!store.sendingRequest)
    }

    @Test func requestActionsUseTheHandleAndSettleOnTheNextSnapshot() throws {
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

    @Test func friendActionsUseTheFriendHandle() throws {
        try deliverSnapshot()
        let ana = try #require(store.friend("id-ana"))
        let bo = try #require(store.friend("id-bo"))
        store.trust(bo)
        store.remove(ana)
        #expect(outbox.types == ["friends.identity.trust", "friends.remove"])
        #expect(outbox.sent.map { $0["username"] } == [.string("bo"), .string("ana")])
    }

    @Test func verifySendsTheTrimmedInvite() throws {
        try deliverSnapshot()
        let bo = try #require(store.friend("id-bo"))
        store.verify(bo, invite: " \n ")
        #expect(outbox.sent.isEmpty)
        store.verify(bo, invite: "  kodosi:bo:abcd\n")
        let command = try #require(outbox.sent.last)
        #expect(command["type"] == .string("friends.verify"))
        #expect(command["username"] == .string("bo"))
        #expect(command["invite"] == .string("kodosi:bo:abcd"))
    }

    @Test func inviteRequestKeepsTheInviteText() throws {
        store.requestInvite()
        store.requestInvite()
        #expect(outbox.types == ["friends.invite"])
        #expect(store.fetchingInvite)
        try store.receive(event("friends.result", ["operation": "friends.invite", "requestId": "request-1", "text": "kodosi:me:0a1b"]))
        #expect(store.ownInvite == "kodosi:me:0a1b")
        #expect(!store.fetchingInvite)
    }

    @Test func onlyFriendErrorsSettlePendingWork() throws {
        try deliverSnapshot()
        let ana = try #require(store.friend("id-ana"))
        store.remove(ana)
        try store.absorb(error: event("devices.error", ["operation": "devices.revoke", "message": "No"]))
        #expect(store.isBusy("ana"))
        try store.absorb(error: event("friends.error", ["operation": "friends.remove", "message": "No"]))
        #expect(!store.isBusy("ana"))
    }

    @Test func resetForgetsTheEarlierAccount() throws {
        try deliverSnapshot()
        try store.receive(event("friends.result", ["operation": "friends.invite", "requestId": "request-1", "text": "kodosi:me:0a1b"]))
        store.reset(userId: "user-2")
        #expect(store.userId == "user-2")
        #expect(store.isEmpty)
        #expect(!store.loaded)
        #expect(store.ownInvite == nil)
    }

    @Test func refreshWaitsForTheNextSnapshot() async throws {
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
                ["userId": "id-ana", "handle": "ana", "displayName": "Ana", "verified": true, "identityState": "fixed"],
                ["userId": "id-bo", "handle": "bo", "displayName": "", "verified": true, "identityState": "changed"],
            ],
            "incoming": [["userId": "id-cy", "handle": "cy", "displayName": "Cy", "createdAt": "2026-10-11T10:00:00Z"]],
            "outgoing": [["userId": "id-di", "handle": "di", "displayName": "Di", "createdAt": "2026-10-11T11:00:00Z"]],
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
