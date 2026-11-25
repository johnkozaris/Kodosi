import Foundation
import KodosiBridge

extension AppDependencies {
    func removeFromProject(_ target: RuntimeSession) async throws {
        try await mutateSession("session.attachProject", session: target, fields: ["projectId": .null])
    }

    func share(_ id: String, with projectId: String) -> Bool {
        guard let target = session(id), target.projectId != projectId else { return false }
        guard target.isOwner else {
            errorMessage = String(localized: "Only the owner of \(target.name) can share it with a project.")
            return false
        }
        Task {
            do {
                try await mutateSession("session.attachProject", session: target, fields: ["projectId": .string(projectId)])
                openProject(id: projectId)
                projectWorkspace(projectId).shownSection = 0
                activateSession(id, inProject: projectId)
            } catch RuntimeError.refused {
            } catch {
                errorMessage = error.localizedDescription
            }
        }
        return true
    }

    func place(_ id: String, at target: String) -> Bool {
        guard id != target, workbench.stagedSessionIds.contains(target) else { return false }
        if !workbench.stagedSessionIds.contains(id) {
            activateSession(id)
        }
        guard workbench.stagedSessionIds.contains(id) else { return false }
        workbench.moveStaged(id, to: target)
        workbench.selectSession(id)
        return true
    }
}
