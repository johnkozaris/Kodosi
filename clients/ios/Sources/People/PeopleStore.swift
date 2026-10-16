import Foundation
import KodosiBridge
import Observation

@Observable
final class PeopleStore {
    private(set) var friends: [FriendEntry] = []
    private(set) var incoming: [FriendRequestEntry] = []
    private(set) var outgoing: [FriendRequestEntry] = []
    private(set) var ownInvite: String?
    private(set) var userId: String?
    private(set) var loaded = false
    private(set) var revision = 0
    private(set) var inFlight: Set<String> = []
    private(set) var sendingRequest = false
    private(set) var fetchingInvite = false

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private var settleTask: Task<Void, Never>?

    init(commands: CommandSink) {
        self.commands = commands
    }

    var isEmpty: Bool {
        friends.isEmpty && incoming.isEmpty && outgoing.isEmpty
    }

    func reset(userId: String?) {
        self.userId = userId
        friends = []
        incoming = []
        outgoing = []
        ownInvite = nil
        loaded = false
        settle()
    }

    func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "friends.snapshot":
            friends = try event.value("friends")
            incoming = try event.value("incoming")
            outgoing = try event.value("outgoing")
            loaded = true
            revision += 1
            settle()
        case "friends.invite":
            ownInvite = event.string("text")
            fetchingInvite = false
        default:
            break
        }
    }

    func absorb(error event: RuntimeEvent) {
        guard event.type == "friends.error" else { return }
        settle()
    }

    func name(of id: String) -> String? {
        friend(id)?.personName
    }

    func friend(_ id: String) -> FriendEntry? {
        friends.first { $0.userId == id }
    }

    func isBusy(_ handle: String) -> Bool {
        inFlight.contains(handle.lowercased())
    }

    func refresh() async {
        let before = revision
        commands.perform("friends.refresh")
        await RefreshWait.until { revision != before }
    }

    func requestInvite() {
        guard !fetchingInvite else { return }
        fetchingInvite = true
        commands.perform("friends.invite")
        armSettle()
    }

    @discardableResult
    func add(_ text: String) -> Bool {
        let username = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !username.isEmpty else { return false }
        sendingRequest = true
        commands.perform("friends.request.send", ["username": .string(username), "requestId": .string(UUIDv7.generate())])
        armSettle()
        return true
    }

    func accept(_ request: FriendRequestEntry) {
        act("friends.request.accept", on: request.handle)
    }

    func decline(_ request: FriendRequestEntry) {
        act("friends.request.reject", on: request.handle)
    }

    func cancel(_ request: FriendRequestEntry) {
        act("friends.request.cancel", on: request.handle)
    }

    func remove(_ friend: FriendEntry) {
        act("friends.remove", on: friend.handle)
    }

    func trust(_ friend: FriendEntry) {
        act("friends.identity.trust", on: friend.handle)
    }

    func verify(_ friend: FriendEntry, invite: String) {
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
