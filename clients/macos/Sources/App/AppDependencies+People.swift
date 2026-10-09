import Foundation

extension AppDependencies {
    var selfName: String {
        let name = displayName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return name.isEmpty ? String(localized: "You") : name
    }

    func personName(_ id: String) -> String {
        if id == userId {
            return selfName
        }
        if let friend = friends.first(where: { $0.userId == id }) {
            return friend.displayName.isEmpty ? friend.handle : friend.displayName
        }
        if let member = missionDetail?.members.first(where: { $0.userId == id }) {
            return member.displayName.isEmpty ? member.handle : member.displayName
        }
        return String(localized: "Guest")
    }

    func viewers(of session: RuntimeSession) -> [AvatarStack.Person] {
        (session.connectedUsers ?? []).filter { $0 != userId }.map { AvatarStack.Person(id: $0, name: personName($0)) }
    }

    func activity(of session: RuntimeSession) -> AgentMark.Activity {
        if session.isWorking {
            return .working
        }
        return session.isConnected && workbench.stagedSessionIds.contains(session.id) ? .awake : .asleep
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

    func close(_ session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await mutateSession("session.close", session: session)
            } catch { errorMessage = error.localizedDescription }
        }
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
