import Foundation
import KodosiBridge
import KodosiDesign

extension AppDependencies {
    func personName(_ id: String) -> String {
        if id == account.userId {
            return account.selfName
        }
        if let name = people.name(of: id) {
            return name
        }
        if let member = missionDetail?.members.first(where: { $0.userId == id }) {
            return member.personName
        }
        return String(localized: "Guest")
    }

    func viewers(of session: RuntimeSession) -> [AvatarStack.Person] {
        (session.connectedUsers ?? []).filter { $0 != account.userId }.map { AvatarStack.Person(id: $0, name: personName($0)) }
    }

    func activity(of session: RuntimeSession) -> AgentMark.Activity {
        session.mark(rested: session.isConnected && workbench.stagedSessionIds.contains(session.id) ? .awake : .asleep)
    }

    func start(_ agent: String, in session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await mutateSession("session.run", session: session, fields: ["agent": AgentStart(agent: agent).json])
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func sign(of session: RuntimeSession) -> StatusSign.Form? {
        session.sign(unseen: session.attention)
    }

    func rename(_ session: RuntimeSession, to name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name), name != session.name else { return }
        Task { @MainActor in
            do {
                try await mutateSession("session.rename", session: session, fields: ["name": .string(name)])
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func close(_ session: RuntimeSession) async {
        do {
            try await mutateSession("session.close", session: session)
        } catch { errorMessage = error.localizedDescription }
    }

    func showRooms() {
        missionTask?.cancel(); missionTask = nil; missionRequestId = nil
        workbench.selectedMissionId = nil
        missionDetail = nil
        workbench.section = .missions
    }
}

extension AppDependencies {
    func respond(to invitation: MissionInvitationEntry, accept: Bool) {
        Task { @MainActor in
            do {
                let operation = accept ? "mission.invitation.accept" : "mission.invitation.reject"
                _ = try await commandSink.request(operation, ["invitationId": .string(invitation.id)])
                if accept {
                    workbench.section = .missions
                    openMission(id: invitation.missionId)
                }
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}
