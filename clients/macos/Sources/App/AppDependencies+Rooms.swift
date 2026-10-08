import Foundation

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

    func newRoomTerminal(_ roomId: String, directory: String? = nil) {
        let request = UUIDv7.generate()
        roomCreations[request] = roomId
        roomView(roomId).canvas = 0
        Task { @MainActor in
            do {
                var fields: [String: JSONValue] = ["requestId": .string(request), "name": .string(generateSessionName()), "missionId": .string(roomId)]
                if let directory = directory ?? settings.lastWorkingDir ?? storageBootstrap.defaultWorkingDirectory?.path {
                    fields["workingDir"] = .string(directory)
                }
                _ = try await commandSink.request("session.create", fields)
            } catch {
                roomCreations.removeValue(forKey: request)
                roomView(roomId).failure = error.localizedDescription
            }
        }
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
                let added = next.messages.filter { entry in !previous.messages.contains { $0.id == entry.id } }.count
                if !state.followsLatest || !state.conversationVisible || workbench.section != .missions || workbench.selectedMissionId != next
                    .roomId
                {
                    state.unread += added
                }
                next.hasOlder = previous.hasOlder && next.hasOlder
                var messages = Dictionary(uniqueKeysWithValues: previous.messages.map { ($0.id, $0) })
                for message in next.messages {
                    messages[message.id] = message
                }
                next.messages = messages.values.sorted { $0.sequence < $1.sequence }
                var tasks = Dictionary(uniqueKeysWithValues: previous.tasks.map { ($0.id, $0) })
                for task in next.tasks where tasks[task.id].map({ $0.version <= task.version }) ?? true {
                    tasks[task.id] = task
                }
                next.tasks = tasks.values.sorted { $0.id < $1.id }
            }
            rooms[next.roomId] = next
        case "room.issues":
            guard let repository = event.string("repositoryId") else { return }
            roomIssues[repository] = try event.value("issues")
        default: break
        }
    }
}
