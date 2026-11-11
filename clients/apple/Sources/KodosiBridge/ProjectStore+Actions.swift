import Foundation

public extension ProjectStore {
    func post(in projectId: String) async {
        let state = view(projectId)
        let text = state.message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !state.sending else { return }
        let revision = state.messageRevision
        state.sending = true
        state.failure = nil
        state.followsLatest = true
        defer { state.sending = false }
        do {
            try await act(projectId, ProjectAction.post(text))
            if state.messageRevision == revision {
                state.message = ""
            }
            state.sent += 1
        } catch is CancellationError {
            return
        } catch {
            state.failure = error.localizedDescription
        }
    }

    func createTask(
        in projectId: String, title: String, description: String, repositoryIds: Set<String>, issueRepositoryId: String? = nil,
    ) async throws {
        guard !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        try await act(projectId, ProjectAction.createTask(
            title: title, description: description, repositoryIds: repositoryIds, issueRepositoryId: issueRepositoryId,
        ))
    }

    func makeTask(from message: ProjectMessage, in projectId: String) async throws {
        try await act(projectId, ProjectAction.createTask(from: message))
    }

    func update(_ task: ProjectTask, in projectId: String, change: ProjectTaskChange, note: String? = nil) async throws {
        try await act(projectId, ProjectAction.updateTask(task.id, change: change, note: note))
    }

    func addRepository(_ address: String, to projectId: String) async throws {
        guard !address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        try await act(projectId, ProjectAction.addRepository(address))
    }

    func create(name: String, inviting friends: [String]) async throws -> String {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name) else {
            throw RuntimeError.operation(String(localized: "Use a shorter name.", bundle: .module))
        }
        let projectId = UUIDv7.generate()
        _ = try await commands.request("project.create", ["requestId": .string(projectId), "name": .string(name)])
        var missed = 0
        for friend in friends {
            do {
                try await invite(friend, to: projectId)
            } catch {
                missed += 1
            }
        }
        if missed > 0 {
            notify(String(localized: "Some invitations did not go out. Invite them again from the project.", bundle: .module))
        }
        return projectId
    }

    func invite(_ userId: String, to projectId: String) async throws {
        _ = try await commands.request("project.invite", ["projectId": .string(projectId), "userId": .string(userId)])
    }

    func respond(to invitation: ProjectInvitationEntry, accept: Bool) async throws {
        let operation = accept ? "project.invitation.accept" : "project.invitation.reject"
        _ = try await commands.request(operation, ["invitationId": .string(invitation.id)])
    }

    func rename(_ projectId: String, to name: String) async throws {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name != project(projectId)?.name else { return }
        guard ProductInput.validName(name) else {
            throw RuntimeError.operation(String(localized: "Use a shorter name.", bundle: .module))
        }
        _ = try await commands.request("project.rename", ["projectId": .string(projectId), "name": .string(name)])
    }

    func delete(_ projectId: String) async throws {
        _ = try await commands.request("project.delete", ["projectId": .string(projectId)])
    }

    func leave(_ projectId: String) async throws {
        _ = try await commands.request("project.leave", ["projectId": .string(projectId)])
    }

    func remove(_ member: ProjectMemberEntry, from projectId: String) async throws {
        _ = try await commands.request("project.removeMember", ["projectId": .string(projectId), "userId": .string(member.userId)])
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
        notify(error.localizedDescription)
    }
}
