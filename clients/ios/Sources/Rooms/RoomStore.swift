import Foundation
import KodosiBridge
import Observation

@Observable
final class RoomStore {
    private(set) var missions: [MissionEntry] = []
    private(set) var invitations: [MissionInvitationEntry] = []
    private(set) var listTruncated = false
    private(set) var listed = false
    private(set) var userId: String?
    private(set) var snapshots: [String: RoomSnapshot] = [:]
    private(set) var details: [String: MissionDetail] = [:]
    private(set) var detailFailures: [String: String] = [:]
    private(set) var issues: [String: [RoomIssue]] = [:]
    private(set) var unread: [String: Int] = [:]
    @ObservationIgnored private var seen: [String: UInt64] = [:]
    private(set) var departed: Set<String> = []
    var visibleRoomId: String?

    @ObservationIgnored let commands: CommandSink
    @ObservationIgnored let notices: NoticeCenter
    @ObservationIgnored private var views: [String: RoomViewState] = [:]
    @ObservationIgnored private var detailLoads: Set<String> = []
    @ObservationIgnored private var listWaiters: [UUID: CheckedContinuation<Void, Never>] = [:]
    @ObservationIgnored private(set) var generation = 0

    init(commands: CommandSink, notices: NoticeCenter) {
        self.commands = commands
        self.notices = notices
    }

    func reset(userId: String?) {
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
        unread = [:]
        seen = [:]
        departed = []
        visibleRoomId = nil
        views = [:]
        detailLoads = []
        finishListWaiters()
    }

    func view(_ roomId: String) -> RoomViewState {
        if let state = views[roomId] {
            return state
        }
        let state = RoomViewState()
        views[roomId] = state
        return state
    }

    func mission(_ roomId: String) -> MissionEntry? {
        details[roomId]?.mission ?? missions.first { $0.id == roomId }
    }

    func isOwner(of roomId: String) -> Bool {
        guard let userId, let mission = mission(roomId) else { return false }
        return mission.ownerUserId == userId
    }

    func isOwn(_ message: RoomMessage) -> Bool {
        message.isWritten(by: userId)
    }

    func markRead(_ roomId: String) {
        if unread[roomId] != nil {
            unread[roomId] = nil
        }
        guard let sequence = snapshots[roomId]?.messages.map(\.sequence).max(),
              mission(roomId)?.unread == true || sequence > seen[roomId] ?? 0 else { return }
        seen[roomId] = sequence
        commands.perform("mission.seen", ["missionId": .string(roomId), "sequence": .uint(sequence)])
    }

    func receive(_ event: RuntimeEvent) throws {
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
                try receiveRoom(event.value("room"))
            }
        default:
            break
        }
    }

    func refreshList() async {
        let sent = commands.send("mission.list")
        guard CommandSink.didAcceptSynchronously(sent) else {
            notices.show(sent.map(RuntimeError.rejected) ?? RuntimeError.unavailable)
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

    func open(_ roomId: String) async {
        async let detail: Void = openDetail(roomId)
        async let messages: Void = read(roomId)
        _ = await (detail, messages)
    }

    func loadDetail(_ roomId: String) async throws {
        guard !detailLoads.contains(roomId) else { return }
        let generation = generation
        detailLoads.insert(roomId)
        detailFailures[roomId] = nil
        defer {
            if generation == self.generation {
                detailLoads.remove(roomId)
            }
        }
        do {
            let reply = try await commands.request("mission.open", ["missionId": .string(roomId)])
            guard generation == self.generation else { return }
            let mission: MissionEntry = try reply.value("mission")
            details[roomId] = try MissionDetail(mission: mission, members: reply.value("members"))
        } catch {
            if generation == self.generation, !(error is CancellationError) {
                detailFailures[roomId] = error.localizedDescription
            }
            throw error
        }
    }

    func prefetchDetails() async {
        for room in missions.prefix(24) where details[room.id] == nil && detailFailures[room.id] == nil {
            do {
                try await loadDetail(room.id)
            } catch {
                return
            }
        }
    }

    func read(_ roomId: String, before: UInt64? = nil) async {
        let state = view(roomId)
        guard !state.loading else { return }
        state.loading = true
        state.readFailure = nil
        defer { state.loading = false }
        do {
            try await act(roomId, RoomAction.read(before: before))
        } catch {
            if !(error is CancellationError) {
                state.readFailure = error.localizedDescription
            }
        }
    }

    func act(_ roomId: String, _ action: [String: JSONValue]) async throws {
        _ = try await commands.request("room.command", ["roomId": .string(roomId), "action": .object(action)])
    }

    private func openDetail(_ roomId: String) async {
        try? await loadDetail(roomId)
    }

    private func receiveList(_ event: RuntimeEvent) throws {
        let received: [MissionEntry] = try event.value("missions")
        let receivedInvitations: [MissionInvitationEntry] = try event.value("invitations")
        let truncated = event.payload["missionsTruncated"] == .bool(true)
        let before = Set(missions.map(\.id))
        let present = Set(received.map(\.id))
        missions = received
        invitations = receivedInvitations
        listed = true
        listTruncated = truncated || event.payload["invitationsTruncated"] == .bool(true)
        departed.subtract(present)
        if !truncated {
            forget(before.subtracting(present))
        }
        if let visible = visibleRoomId {
            if present.contains(visible) {
                Task { try? await loadDetail(visible) }
            } else if before.contains(visible), !truncated {
                departed.insert(visible)
            }
        }
        finishListWaiters()
    }

    private func forget(_ rooms: Set<String>) {
        guard !rooms.isEmpty else { return }
        for room in rooms {
            snapshots[room] = nil
            details[room] = nil
            detailFailures[room] = nil
            unread[room] = nil
        }
    }

    private func receiveRoom(_ received: RoomSnapshot) {
        var next = received
        let roomId = next.roomId
        if let previous = snapshots[roomId] {
            let arrived = next.newMessages(after: previous, userId: userId)
            let state = views[roomId]
            let visible = visibleRoomId == roomId
            if arrived > 0, !(visible && state?.reading == true) {
                unread[roomId, default: 0] += arrived
            }
            next.merge(previous)
            if previous.tasks.map(\.version) != next.tasks.map(\.version), !(visible && state?.watchingTasks == true) {
                view(roomId).freshTasks = true
            }
        }
        snapshots[roomId] = next
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
