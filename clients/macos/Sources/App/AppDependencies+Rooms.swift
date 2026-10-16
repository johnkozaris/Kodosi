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
        case "room.issues":
            guard let repository = event.string("repositoryId") else { return }
            roomIssues[repository] = try event.value("issues")
        default: break
        }
    }
}
