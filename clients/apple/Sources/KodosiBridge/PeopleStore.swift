import Foundation
import Observation

@MainActor
@Observable
public final class PeopleStore {
    public private(set) var friends: [FriendEntry] = []
    public private(set) var incoming: [FriendRequestEntry] = []
    public private(set) var outgoing: [FriendRequestEntry] = []
    public private(set) var ownInvite: String?
    public private(set) var userId: String?
    public private(set) var loaded = false
    public private(set) var revision = 0
    public private(set) var inFlight: Set<String> = []
    public private(set) var sendingRequest = false
    public private(set) var fetchingInvite = false

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private var settleTask: Task<Void, Never>?

    public init(commands: CommandSink) {
        self.commands = commands
    }

    public var isEmpty: Bool {
        friends.isEmpty && incoming.isEmpty && outgoing.isEmpty
    }

    public func reset(userId: String?) {
        self.userId = userId
        friends = []
        incoming = []
        outgoing = []
        ownInvite = nil
        loaded = false
        settle()
    }

    public func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "friends.snapshot":
            friends = try event.value("friends")
            incoming = try event.value("incoming")
            outgoing = try event.value("outgoing")
            loaded = true
            revision += 1
            settle()
        case "friends.result" where event.string("operation") == "friends.invite":
            ownInvite = event.string("text")
            fetchingInvite = false
        default:
            break
        }
    }

    public func absorb(error event: RuntimeEvent) {
        guard event.type == "friends.error" else { return }
        settle()
    }

    public func name(of id: String) -> String? {
        friend(id)?.personName
    }

    public func friend(_ id: String) -> FriendEntry? {
        friends.first { $0.userId == id }
    }

    public func isBusy(_ handle: String) -> Bool {
        inFlight.contains(handle.lowercased())
    }

    public func refresh() async {
        let before = revision
        commands.perform("friends.refresh")
        await RefreshWait.until { revision != before }
    }

    public func requestInvite() {
        guard !fetchingInvite else { return }
        fetchingInvite = true
        commands.perform("friends.invite")
        armSettle()
    }

    @discardableResult
    public func add(_ text: String) -> Bool {
        let username = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !username.isEmpty else { return false }
        sendingRequest = true
        commands.perform("friends.request.send", ["username": .string(username), "requestId": .string(UUIDv7.generate())])
        armSettle()
        return true
    }

    public func accept(_ request: FriendRequestEntry) {
        act("friends.request.accept", on: request.handle)
    }

    public func decline(_ request: FriendRequestEntry) {
        act("friends.request.reject", on: request.handle)
    }

    public func cancel(_ request: FriendRequestEntry) {
        act("friends.request.cancel", on: request.handle)
    }

    public func remove(_ friend: FriendEntry) {
        act("friends.remove", on: friend.handle)
    }

    public func trust(_ friend: FriendEntry) {
        act("friends.identity.trust", on: friend.handle)
    }

    public func verify(_ friend: FriendEntry, invite: String) {
        let invite = invite.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !invite.isEmpty else { return }
        act("friends.verify", on: friend.handle, ["invite": .string(invite)])
    }

    private func act(_ type: String, on handle: String, _ fields: [String: JSONValue] = [:]) {
        guard !isBusy(handle) else { return }
        var fields = fields
        fields["username"] = .string(handle)
        inFlight.insert(handle.lowercased())
        commands.perform(type, fields)
        armSettle()
    }

    private func armSettle() {
        settleTask?.cancel()
        settleTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(20))
            guard !Task.isCancelled else { return }
            self?.settle()
        }
    }

    private func settle() {
        settleTask?.cancel()
        settleTask = nil
        inFlight = []
        sendingRequest = false
        fetchingInvite = false
    }
}
