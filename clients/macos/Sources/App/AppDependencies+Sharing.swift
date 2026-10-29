import Foundation
import KodosiBridge

extension AppDependencies {
    func removeFromRoom(_ target: RuntimeSession) async throws {
        try await mutateSession("session.attachMission", session: target, fields: ["missionId": .null])
    }

    func askToShare(_ id: String, with roomId: String) -> Bool {
        guard let target = session(id), target.missionId != roomId else { return false }
        guard target.isOwner else {
            errorMessage = String(localized: "Only the owner of \(target.name) can share it with a room.")
            return false
        }
        workbench.roomShare = RoomShare(sessionId: id, roomId: roomId)
        return true
    }

    func share(_ share: RoomShare) {
        guard let target = session(share.sessionId) else { return }
        Task { @MainActor in
            do {
                try await mutateSession("session.attachMission", session: target, fields: ["missionId": .string(share.roomId)])
                if workbench.selectedMissionId != share.roomId || missionDetail?.mission.id != share.roomId {
                    openMission(id: share.roomId)
                }
                roomView(share.roomId).shownSection = 0
                activateSession(share.sessionId, inRoom: share.roomId)
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
