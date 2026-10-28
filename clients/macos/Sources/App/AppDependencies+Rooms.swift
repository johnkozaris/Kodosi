import Foundation
import KodosiBridge

extension AppDependencies {
    func postRoomMessage(_ roomId: String) async {
        let state = roomView(roomId)
        let text = state.message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !state.busy else { return }
        let revision = state.messageRevision
        state.busy = true; state.failure = nil
        defer { state.busy = false }
        do {
            try await roomAction(roomId, RoomAction.post(text))
            if state.messageRevision == revision {
                state.message = ""
            }
        } catch { state.failure = error.localizedDescription }
    }

    func roomView(_ roomId: String) -> RoomViewState {
        if let state = roomViews[roomId] {
            return state
        }
        let state = RoomViewState()
        roomViews[roomId] = state
        return state
    }

    func readRoom(_ roomId: String, before: UInt64? = nil) async {
        let state = roomView(roomId)
        guard !state.loading else { return }
        state.loading = true
        state.failure = nil
        defer { state.loading = false }
        do {
            try await roomAction(roomId, RoomAction.read(before: before))
        } catch { state.failure = error.localizedDescription }
    }

    func roomAction(_ roomId: String, _ action: [String: JSONValue]) async throws {
        _ = try await commandSink.request("room.command", ["roomId": .string(roomId), "action": .object(action)])
    }

    func repositoryChoices(for roomId: String) async throws -> RepositoryChoices {
        try await commandSink.request("repositories.list", ["missionId": .string(roomId)]).value("repositories")
    }

    func connectGitHub() {
        Task { @MainActor in
            do {
                try await createSession(name: String(localized: "GitHub sign-in"), directory: settings.lastWorkingDir, command: "gh auth login")
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func receiveRoom(_ event: RuntimeEvent) throws {
        switch event.type {
        case "room.snapshot":
            var next: RoomSnapshot = try event.value("room")
            if let previous = rooms[next.roomId] {
                let state = roomView(next.roomId)
                let added = next.newMessages(after: previous, userId: account.userId)
                let watching = workbench.section == .missions && workbench.selectedMissionId == next.roomId
                if !state.followsLatest || !state.conversationVisible || !watching {
                    state.unread += added
                }
                next.merge(previous)
                if previous.tasks.map(\.version) != next.tasks.map(\.version), !(watching && state.canvas == 1) {
                    state.fresh.insert(1)
                }
                if previous.repositories.map(\.id) != next.repositories.map(\.id), !(watching && state.canvas == 2) {
                    state.fresh.insert(2)
                }
            }
            rooms[next.roomId] = next
            if workbench.selectedMissionId == next.roomId {
                seeShownRoom()
            }
        case "room.result":
            try receiveRoomResult(event)
        default: break
        }
    }

    private func receiveRoomResult(_ event: RuntimeEvent) throws {
        if let repository = event.string("repositoryId") {
            roomIssues[repository] = try event.value("issues")
        }
        if event.payload["room"] != nil {
            var page: RoomSnapshot = try event.value("room")
            if let previous = rooms[page.roomId] {
                page.merge(previous)
            }
            rooms[page.roomId] = page
        }
    }

    func seeShownRoom() {
        guard workbench.section == .missions, let roomId = workbench.selectedMissionId,
              roomView(roomId).followsLatest, roomView(roomId).conversationVisible else { return }
        seeRoom(roomId)
    }

    func seeRoom(_ roomId: String) {
        guard isFront(), let sequence = rooms[roomId]?.messages.map(\.sequence).max(),
              sequence > roomSeen[roomId] ?? 0 else { return }
        roomSeen[roomId] = sequence
        commandSink.perform("mission.seen", ["missionId": .string(roomId), "sequence": .uint(sequence)])
    }
}
