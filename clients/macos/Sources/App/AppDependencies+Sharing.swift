import Foundation
import KodosiBridge

extension AppDependencies {
    func removeFromProject(_ target: RuntimeSession) async throws {
        try await mutateSession("session.attachProject", session: target, fields: ["projectId": .null])
    }

    func askToShare(_ id: String, with projectId: String) -> Bool {
        guard let target = session(id), target.projectId != projectId else { return false }
        guard target.isOwner else {
            errorMessage = String(localized: "Only the owner of \(target.name) can share it with a project.")
            return false
        }
        workbench.projectShare = ProjectShare(sessionId: id, projectId: projectId)
        return true
    }

    func share(_ share: ProjectShare) {
        guard let target = session(share.sessionId) else { return }
        Task { @MainActor in
            do {
                try await mutateSession("session.attachProject", session: target, fields: ["projectId": .string(share.projectId)])
                openProject(id: share.projectId)
                projectWorkspace(share.projectId).shownSection = 0
                activateSession(share.sessionId, inProject: share.projectId)
            } catch { errorMessage = error.localizedDescription }
        }
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
