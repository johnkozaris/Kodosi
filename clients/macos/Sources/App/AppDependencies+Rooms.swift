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
            try await roomAction(roomId, ["type": .string("post"), "text": .string(text)])
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
            var action: [String: JSONValue] = ["type": .string("read")]
            if let before {
                action["before"] = .uint(before)
            }
            try await roomAction(roomId, action)
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
                let latest = previous.messages.map(\.sequence).max() ?? 0
                let added = next.messages.count { $0.sequence > latest && !($0.agent == nil && $0.authorId == userId) }
                if !state.followsLatest || !state.conversationVisible || workbench.section != .missions || workbench.selectedMissionId != next
                    .roomId
                {
                    state.unread += added
                }
                next.merge(previous)
                let watching = workbench.section == .missions && workbench.selectedMissionId == next.roomId
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
