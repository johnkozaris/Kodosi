import Foundation
import KodosiBridge

extension AppDependencies {
    func projectWorkspace(_ projectId: String) -> ProjectWorkspace {
        if let workspace = projectWorkspaces[projectId] {
            return workspace
        }
        let workspace = ProjectWorkspace()
        projectWorkspaces[projectId] = workspace
        return workspace
    }

    func repositoryChoices(for projectId: String) async throws -> RepositoryChoices {
        try await commandSink.request("repositories.list", ["projectId": .string(projectId)]).value("repositories")
    }

    func connectGitHub() {
        Task { @MainActor in
            do {
                try await createSession(name: String(localized: "GitHub sign-in"), directory: settings.lastWorkingDir, command: "gh auth login")
            } catch { errorMessage = error.localizedDescription }
        }
    }
}
