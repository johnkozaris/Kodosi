import Foundation
import KodosiBridge

extension RoomStore {
    func post(in roomId: String) async {
        let state = view(roomId)
        let text = state.message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !state.sending else { return }
        let revision = state.messageRevision
        state.sending = true
        state.failure = nil
        defer { state.sending = false }
        do {
            try await act(roomId, ["type": .string("post"), "text": .string(text)])
            if state.messageRevision == revision {
                state.message = ""
            }
            state.sent += 1
        } catch {
            if !(error is CancellationError) {
                state.failure = error.localizedDescription
            }
        }
    }

    func createTask(in roomId: String, title: String, description: String, repositoryIds: Set<String>) async throws {
        guard !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        try await act(roomId, RoomTaskAction.create(title: title, description: description, repositoryIds: repositoryIds))
    }

    func makeTask(from message: RoomMessage, in roomId: String) async throws {
        try await act(roomId, RoomTaskAction.create(from: message))
    }

    func update(_ task: RoomTask, in roomId: String, change: RoomTaskChange, note: String? = nil) async throws {
        var action: [String: JSONValue] = [
            "type": .string("updateTask"),
            "taskId": .string(task.id),
            "change": .string(change.rawValue),
        ]
        if let note = note?.trimmingCharacters(in: .whitespacesAndNewlines), !note.isEmpty {
            action["note"] = .string(note)
        }
        try await act(roomId, action)
    }

    func addRepository(_ address: String, to roomId: String) async throws {
        let address = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !address.isEmpty else { return }
        try await act(roomId, ["type": .string("addRepository"), "url": .string(address)])
    }

    func create(name: String, inviting friends: [String]) async throws -> String {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name) else {
            throw RuntimeError.operation(String(localized: "Use a shorter name."))
        }
        let roomId = UUIDv7.generate()
        _ = try await commands.request("mission.create", ["requestId": .string(roomId), "name": .string(name)])
        var missed = 0
        for friend in friends {
            do {
                try await invite(friend, to: roomId)
            } catch {
                missed += 1
            }
        }
        if missed > 0 {
            notices.show(String(localized: "Some invitations did not go out. Invite them again from the room."))
        }
        return roomId
    }

    func invite(_ userId: String, to roomId: String) async throws {
        _ = try await commands.request("mission.invite", ["missionId": .string(roomId), "userId": .string(userId)])
    }

    func respond(to invitation: MissionInvitationEntry, accept: Bool) async throws {
        let operation = accept ? "mission.invitation.accept" : "mission.invitation.reject"
        _ = try await commands.request(operation, ["invitationId": .string(invitation.id)])
    }

    func rename(_ roomId: String, to name: String) async throws {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name), name != mission(roomId)?.name else { return }
        _ = try await commands.request("mission.rename", ["missionId": .string(roomId), "name": .string(name)])
    }

    func delete(_ roomId: String) async throws {
        _ = try await commands.request("mission.delete", ["missionId": .string(roomId)])
    }

    func leave(_ roomId: String) async throws {
        _ = try await commands.request("mission.leave", ["missionId": .string(roomId)])
    }

    func remove(_ member: MissionMemberEntry, from roomId: String) async throws {
        _ = try await commands.request("mission.removeMember", ["missionId": .string(roomId), "userId": .string(member.userId)])
    }

    func run(_ work: @escaping () async throws -> Void) {
        Task {
            do {
                try await work()
            } catch {
                report(error)
            }
        }
    }

    func report(_ error: any Error) {
        guard !(error is CancellationError) else { return }
        notices.show(error)
    }
}
