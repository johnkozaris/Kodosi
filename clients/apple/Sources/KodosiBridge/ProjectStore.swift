import Foundation
import Observation

@MainActor
@Observable
public final class ProjectStore {
    public private(set) var entries: [ProjectEntry] = []
    public private(set) var invitations: [ProjectInvitationEntry] = []
    public private(set) var listTruncated = false
    public private(set) var listed = false
    public private(set) var userId: String?
    public private(set) var snapshots: [String: ProjectSnapshot] = [:]
    public private(set) var details: [String: ProjectDetail] = [:]
    public private(set) var detailFailures: [String: String] = [:]
    public private(set) var issues: [String: [ProjectIssue]] = [:]
    /// The projects that the person was in and is no longer in.
    public private(set) var departed: Set<String> = []
    public var visibleProjectId: String?

    @ObservationIgnored public var isFront: () -> Bool = { true }
    @ObservationIgnored public var notify: (String) -> Void = { _ in }
    @ObservationIgnored let commands: CommandSink
    @ObservationIgnored private var views: [String: ProjectViewState] = [:]
    @ObservationIgnored private var seen: [String: UInt64] = [:]
    /// The projects of the lists since the last complete one. A complete list without one of them
    /// means that the person left it.
    @ObservationIgnored private var listedProjects: Set<String> = []
    /// The newest load of the people of each project, so that an older answer does not replace it.
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
        entries = []
        invitations = []
        listTruncated = false
        listed = false
        snapshots = [:]
        details = [:]
        detailFailures = [:]
        issues = [:]
        departed = []
        visibleProjectId = nil
        views = [:]
        seen = [:]
        listedProjects = []
        detailLoads = [:]
        finishListWaiters()
    }

    public func view(_ projectId: String) -> ProjectViewState {
        if let state = views[projectId] {
            return state
        }
        let state = ProjectViewState()
        state.startedReading = { [weak self] in self?.markRead(projectId) }
        views[projectId] = state
        return state
    }

    public func project(_ projectId: String) -> ProjectEntry? {
        details[projectId]?.project ?? entries.first { $0.id == projectId }
    }

    public func isOwner(of projectId: String) -> Bool {
        guard let userId, let project = project(projectId) else { return false }
        return project.ownerUserId == userId
    }

    public func isOwn(_ message: ProjectMessage) -> Bool {
        message.isWritten(by: userId)
    }

    public func member(_ userId: String, in projectId: String) -> ProjectMemberEntry? {
        details[projectId]?.members.first { $0.userId == userId }
    }

    /// A message of your own agent has you as its author, and you cannot block or report yourself.
    public func otherAuthor(of message: ProjectMessage, in projectId: String) -> ProjectMemberEntry? {
        message.authorId == userId ? nil : member(message.authorId, in: projectId)
    }

    public func invitable(_ friends: [FriendEntry], to projectId: String) -> [FriendEntry] {
        let members = Set((details[projectId]?.members ?? []).map(\.userId))
        return friends.filter { !members.contains($0.userId) }
    }

    public func markRead(_ projectId: String) {
        view(projectId).unread = 0
        guard isFront(), let sequence = snapshots[projectId]?.messages.map(\.sequence).max(),
              project(projectId)?.unread == true || sequence > seen[projectId] ?? 0 else { return }
        seen[projectId] = sequence
        commands.perform("project.seen", ["projectId": .string(projectId), "sequence": .uint(sequence)])
    }

    public func markShownRead() {
        for (projectId, state) in views where state.reading {
            markRead(projectId)
        }
    }

    public func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "projects.snapshot":
            try receiveList(event)
        case "project.snapshot":
            try receiveSnapshot(event.value("content"))
        case "project.result":
            if let repository = event.string("repositoryId") {
                issues[repository] = try event.value("issues")
            }
            if event.payload["content"] != nil {
                try receivePage(event.value("content"))
            }
        default:
            break
        }
    }

    public func refreshList() async {
        let sent = commands.send("project.list")
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

    public func open(_ projectId: String) async {
        async let detail: Void = openDetail(projectId)
        async let messages: Void = read(projectId)
        _ = await (detail, messages)
    }

    public func loadDetail(_ projectId: String) async throws {
        let generation = generation
        loads += 1
        let load = loads
        detailLoads[projectId] = load
        detailFailures[projectId] = nil
        defer {
            if generation == self.generation, detailLoads[projectId] == load {
                detailLoads[projectId] = nil
            }
        }
        do {
            let reply = try await commands.request("project.open", ["projectId": .string(projectId)])
            guard generation == self.generation, detailLoads[projectId] == load else { return }
            let project: ProjectEntry = try reply.value("project")
            details[projectId] = try ProjectDetail(project: project, members: reply.value("members"))
        } catch {
            if generation == self.generation, detailLoads[projectId] == load, !(error is CancellationError) {
                detailFailures[projectId] = error.localizedDescription
            }
            throw error
        }
    }

    public func prefetchDetails() async {
        for project in entries.prefix(24)
            where details[project.id] == nil && detailFailures[project.id] == nil && detailLoads[project.id] == nil
        {
            do {
                try await loadDetail(project.id)
            } catch {
                return
            }
        }
    }

    public func read(_ projectId: String, before: UInt64? = nil) async {
        let state = view(projectId)
        guard !state.loading else { return }
        state.loading = true
        state.readFailure = nil
        defer { state.loading = false }
        do {
            try await act(projectId, ProjectAction.read(before: before))
        } catch is CancellationError {
            return
        } catch {
            state.readFailure = error.localizedDescription
        }
    }

    public func act(_ projectId: String, _ action: [String: JSONValue]) async throws {
        _ = try await commands.request("project.act", ["projectId": .string(projectId), "action": .object(action)])
    }

    private func openDetail(_ projectId: String) async {
        try? await loadDetail(projectId)
    }

    private func receiveList(_ event: RuntimeEvent) throws {
        let received: [ProjectEntry] = try event.value("projects")
        let receivedInvitations: [ProjectInvitationEntry] = try event.value("invitations")
        let truncated = event.payload["projectsTruncated"] == .bool(true)
        let present = Set(received.map(\.id))
        entries = received
        invitations = receivedInvitations
        listed = true
        listTruncated = truncated || event.payload["invitationsTruncated"] == .bool(true)
        departed.subtract(present)
        if truncated {
            listedProjects.formUnion(present)
        } else {
            let gone = listedProjects.subtracting(present)
            departed.formUnion(gone)
            forget(gone)
            listedProjects = present
        }
        if let visible = visibleProjectId, !departed.contains(visible), truncated || present.contains(visible) {
            Task { try? await loadDetail(visible) }
        }
        finishListWaiters()
    }

    private func forget(_ projects: Set<String>) {
        for project in projects {
            snapshots[project] = nil
            details[project] = nil
            detailFailures[project] = nil
            views[project]?.unread = 0
        }
    }

    private func receiveSnapshot(_ received: ProjectSnapshot) {
        var next = received
        let projectId = next.projectId
        let state = view(projectId)
        if let previous = snapshots[projectId] {
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
        snapshots[projectId] = next
        if state.reading {
            markRead(projectId)
        }
    }

    private func receivePage(_ page: ProjectSnapshot) {
        var next = page
        if let previous = snapshots[page.projectId] {
            next.merge(previous)
        }
        snapshots[page.projectId] = next
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
