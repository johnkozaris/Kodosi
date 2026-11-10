import Foundation
import KodosiBridge
import KodosiDesign

extension AppDependencies {
    func copyInvite() {
        inviteRequested = Date()
        people.requestInvite()
    }

    func personName(_ id: String) -> String {
        if id == account.userId {
            return account.selfName
        }
        if let name = people.name(of: id) {
            return name
        }
        if let member = rooms.details.values.lazy.flatMap(\.members).first(where: { $0.userId == id }) {
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

    /// A terminal of another computer that Kodosi cannot reach because Kodosi is offline.
    func unreachable(_ session: RuntimeSession) -> Bool {
        session.kind == .remote && !account.online && session.connectionState != .blocked
    }

    func statusLabel(of session: RuntimeSession) -> String {
        unreachable(session) ? String(localized: "Offline") : session.statusLabel
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
        workbench.noteClosing(session.id)
        do {
            try await mutateSession("session.close", session: session)
        } catch {
            workbench.cancelClosing(session.id)
            errorMessage = error.localizedDescription
        }
    }

    func close(_ sessions: [RuntimeSession]) {
        for session in sessions {
            Task { @MainActor in await close(session) }
        }
    }

    func hide(_ sessions: [RuntimeSession]) {
        for session in sessions {
            Task { @MainActor in
                do {
                    try await mutateSession("session.hide", session: session)
                } catch { errorMessage = error.localizedDescription }
            }
        }
    }

    func showRooms() {
        workbench.selectedMissionId = nil
        workbench.section = .missions
    }
}

extension AppDependencies {
    func respond(to invitation: MissionInvitationEntry, accept: Bool) {
        rooms.run { [weak self] in
            try await self?.rooms.respond(to: invitation, accept: accept)
            if accept {
                self?.workbench.section = .missions
                self?.openMission(id: invitation.missionId)
            }
        }
    }
}
