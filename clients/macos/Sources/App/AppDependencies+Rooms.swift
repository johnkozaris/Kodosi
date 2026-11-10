import Foundation
import KodosiBridge

extension AppDependencies {
    func roomWorkspace(_ roomId: String) -> RoomWorkspace {
        if let workspace = roomWorkspaces[roomId] {
            return workspace
        }
        let workspace = RoomWorkspace()
        roomWorkspaces[roomId] = workspace
        return workspace
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
}
