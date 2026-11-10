import Foundation
import Observation

/// The rooms of the account: the list, the conversation, tasks and people of each room, and what
/// the person did not read yet.
@MainActor
@Observable
public final class RoomStore {
    public private(set) var missions: [MissionEntry] = []
    public private(set) var invitations: [MissionInvitationEntry] = []
    public private(set) var listTruncated = false
    public private(set) var listed = false
    public private(set) var userId: String?
    public private(set) var snapshots: [String: RoomSnapshot] = [:]
    public private(set) var details: [String: MissionDetail] = [:]
    public private(set) var detailFailures: [String: String] = [:]
    public private(set) var issues: [String: [RoomIssue]] = [:]
    /// The rooms that the person was in and is no longer in.
    public private(set) var departed: Set<String> = []
    /// The room that the app shows, whose people stay fresh.
    public var visibleRoomId: String?

    /// Whether the app is in front, so that what the person reads there is seen.
    @ObservationIgnored public var isFront: () -> Bool = { true }
    @ObservationIgnored public var notify: (String) -> Void = { _ in }
    @ObservationIgnored let commands: CommandSink
    @ObservationIgnored private var views: [String: RoomViewState] = [:]
    @ObservationIgnored private var seen: [String: UInt64] = [:]
    /// The rooms of the lists since the last complete one. A complete list without one of them
    /// means that the person left it.
    @ObservationIgnored private var listedRooms: Set<String> = []
    /// The newest load of the people of each room, so that an older answer does not replace it.
    @ObservationIgnored private var detailLoads: [String: Int] = [:]
    @ObservationIgnored private var loads = 0
    @ObservationIgnored private var listWaiters: [UUID: CheckedContinuation<Void, Never>] = [:]
    @ObservationIgnored private var generation = 0

    public init(commands: CommandSink) {
        self.commands = commands
    }

    public func reset(userId: String?) {
        generation += 1
        self.userId = userId
        missions = []
        invitations = []
        listTruncated = false
        listed = false
        snapshots = [:]
        details = [:]
        detailFailures = [:]
        issues = [:]
        departed = []
        visibleRoomId = nil
        views = [:]
        seen = [:]
        listedRooms = []
        detailLoads = [:]
        finishListWaiters()
    }

    public func view(_ roomId: String) -> RoomViewState {
        if let state = views[roomId] {
            return state
        }
        let state = RoomViewState()
        state.startedReading = { [weak self] in self?.markRead(roomId) }
        views[roomId] = state
        return state
    }

    public func mission(_ roomId: String) -> MissionEntry? {
        details[roomId]?.mission ?? missions.first { $0.id == roomId }
    }

    public func isOwner(of roomId: String) -> Bool {
        guard let userId, let mission = mission(roomId) else { return false }
        return mission.ownerUserId == userId
    }

    public func isOwn(_ message: RoomMessage) -> Bool {
        message.isWritten(by: userId)
    }

    public func member(_ userId: String, in roomId: String) -> MissionMemberEntry? {
        details[roomId]?.members.first { $0.userId == userId }
    }

    /// The friends whom the person can invite to the room.
    public func invitable(_ friends: [FriendEntry], to roomId: String) -> [FriendEntry] {
        let members = Set((details[roomId]?.members ?? []).map(\.userId))
        return friends.filter { !members.contains($0.userId) }
    }

    /// Clears the count of new messages and, while the app is in front, keeps up to where the
    /// person read, so that the room is no longer unread.
    public func markRead(_ roomId: String) {
        view(roomId).unread = 0
        guard isFront(), let sequence = snapshots[roomId]?.messages.map(\.sequence).max(),
              mission(roomId)?.unread == true || sequence > seen[roomId] ?? 0 else { return }
        seen[roomId] = sequence
        commands.perform("mission.seen", ["missionId": .string(roomId), "sequence": .uint(sequence)])
    }

    /// Marks the rooms whose latest messages are on screen as read, as when the app comes to the
    /// front.
    public func markShownRead() {
        for (roomId, state) in views where state.reading {
            markRead(roomId)
        }
    }

    public func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "missions.snapshot":
            try receiveList(event)
        case "room.snapshot":
            try receiveRoom(event.value("room"))
        case "room.result":
            if let repository = event.string("repositoryId") {
                issues[repository] = try event.value("issues")
            }
            if event.payload["room"] != nil {
                try receivePage(event.value("room"))
            }
        default:
            break
        }
    }

    public func refreshList() async {
        let sent = commands.send("mission.list")
        guard CommandSink.didAcceptSynchronously(sent) else {
            report(sent.map(RuntimeError.rejected) ?? RuntimeError.unavailable)
            return
        }
        let id = UUID()
        let timeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(6))
            self?.finishListWaiter(id)
        }
        await withCheckedContinuation { continuation in
            listWaiters[id] = continuation
        }
        timeout.cancel()
    }

    /// Loads the people and the latest messages of a room.
    public func open(_ roomId: String) async {
        async let detail: Void = openDetail(roomId)
        async let messages: Void = read(roomId)
        _ = await (detail, messages)
    }

    public func loadDetail(_ roomId: String) async throws {
        let generation = generation
        loads += 1
        let load = loads
        detailLoads[roomId] = load
        detailFailures[roomId] = nil
        defer {
            if generation == self.generation, detailLoads[roomId] == load {
                detailLoads[roomId] = nil
            }
        }
        do {
            let reply = try await commands.request("mission.open", ["missionId": .string(roomId)])
            guard generation == self.generation, detailLoads[roomId] == load else { return }
            let mission: MissionEntry = try reply.value("mission")
            details[roomId] = try MissionDetail(mission: mission, members: reply.value("members"))
        } catch {
            if generation == self.generation, detailLoads[roomId] == load, !(error is CancellationError) {
                detailFailures[roomId] = error.localizedDescription
            }
            throw error
        }
    }

    public func prefetchDetails() async {
        for room in missions.prefix(24)
            where details[room.id] == nil && detailFailures[room.id] == nil && detailLoads[room.id] == nil
        {
            do {
                try await loadDetail(room.id)
            } catch {
                return
            }
        }
    }

    public func read(_ roomId: String, before: UInt64? = nil) async {
        let state = view(roomId)
        guard !state.loading else { return }
        state.loading = true
        state.readFailure = nil
        defer { state.loading = false }
        do {
            try await act(roomId, RoomAction.read(before: before))
        } catch is CancellationError {
            return
        } catch {
            state.readFailure = error.localizedDescription
        }
    }

    public func act(_ roomId: String, _ action: [String: JSONValue]) async throws {
        _ = try await commands.request("room.command", ["roomId": .string(roomId), "action": .object(action)])
    }

    private func openDetail(_ roomId: String) async {
        try? await loadDetail(roomId)
    }

    private func receiveList(_ event: RuntimeEvent) throws {
        let received: [MissionEntry] = try event.value("missions")
        let receivedInvitations: [MissionInvitationEntry] = try event.value("invitations")
        let truncated = event.payload["missionsTruncated"] == .bool(true)
        let present = Set(received.map(\.id))
        missions = received
        invitations = receivedInvitations
        listed = true
        listTruncated = truncated || event.payload["invitationsTruncated"] == .bool(true)
        departed.subtract(present)
        if truncated {
            listedRooms.formUnion(present)
        } else {
            let gone = listedRooms.subtracting(present)
            departed.formUnion(gone)
            forget(gone)
            listedRooms = present
        }
        if let visible = visibleRoomId, !departed.contains(visible), truncated || present.contains(visible) {
            Task { try? await loadDetail(visible) }
        }
        finishListWaiters()
    }

    private func forget(_ rooms: Set<String>) {
        for room in rooms {
            snapshots[room] = nil
            details[room] = nil
            detailFailures[room] = nil
            views[room]?.unread = 0
        }
    }

    private func receiveRoom(_ received: RoomSnapshot) {
        var next = received
        let roomId = next.roomId
        let state = view(roomId)
        if let previous = snapshots[roomId] {
            if !state.reading {
                state.unread += next.newMessages(after: previous, userId: userId)
            }
            next.merge(previous)
            if previous.tasks.map(\.version) != next.tasks.map(\.version), !state.shown.contains(.tasks) {
                state.fresh.insert(.tasks)
            }
            if previous.repositories.map(\.id) != next.repositories.map(\.id), !state.shown.contains(.repositories) {
                state.fresh.insert(.repositories)
            }
        }
        snapshots[roomId] = next
        if state.reading {
            markRead(roomId)
        }
    }

    /// Keeps a page of earlier messages, which is no news.
    private func receivePage(_ page: RoomSnapshot) {
        var next = page
        if let previous = snapshots[page.roomId] {
            next.merge(previous)
        }
        snapshots[page.roomId] = next
    }

    private func finishListWaiter(_ id: UUID) {
        listWaiters.removeValue(forKey: id)?.resume()
    }

    private func finishListWaiters() {
        let waiters = Array(listWaiters.values)
        listWaiters.removeAll()
        for waiter in waiters {
            waiter.resume()
        }
    }
}
