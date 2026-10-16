import AppKit
import Foundation
import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions

extension AppDependencies {
    func consumeDeepLink() {
        guard let link = pendingDeepLink else { return }
        guard session(link.sessionId) != nil || (account.userId != nil && !account.stage.isPending) else { return }
        pendingDeepLink = nil
        activateSession(link.sessionId)
    }

    private func acceptAccount(_ event: RuntimeEvent) -> Bool {
        let establishes = event.type == "auth.ready" || event.type == "auth.required" || event.type == "system.ready"
        if !accountKnown || event.accountEpoch > accountEpoch {
            guard establishes else { return false }
            let previousIds = Set(sessions.map(\.id))
            for id in previousIds {
                terminalFocus.releaseSession(sessionId: id); terminalManager.releaseSession(for: id)
            }
            sessions.removeAll()
            terminalNotifications.reconcile(sessions: [], accountEpoch: event.accountEpoch)
            account.reset(userId: event.accountUserId)
            people.reset(userId: event.accountUserId); inviteRequested = nil
            missions.removeAll(); invitations.removeAll()
            missionListTruncated = false
            missionDetail = nil
            rooms.removeAll(); roomIssues.removeAll(); roomViews.removeAll(); roomCreations.removeAll()
            attention.removeAll(); freshTerminals.removeAll()
            activationTasks.values.forEach { $0.cancel() }; activationTasks.removeAll()
            pendingCreations.removeAll()
            missionTask?.cancel(); missionTask = nil; missionRequestId = nil
            accountKnown = true
            accountEpoch = event.accountEpoch
            commandSink.setAccount(userId: account.userId, epoch: accountEpoch)
            workbench.switchAccount(account.userId)
        }
        return event.accountEpoch == accountEpoch && event.accountUserId == account.userId
    }

    func receive(_ event: RuntimeEvent) throws {
        guard acceptAccount(event) else { return }
        commandSink.receive(event)
        if event.type.hasSuffix(".error") {
            receiveError(event)
            return
        }
        let family = event.type.prefix(while: { $0 != "." })
        switch family {
        case "system": try receiveSystem(event)
        case "auth": try receiveAuth(event)
        case "sessions": try receiveSessions(event)
        case "friends": try receiveFriends(event)
        case "devices": try account.receiveDevices(event)
        case "mission", "missions", "room": try receiveCollaboration(event)
        case "term": receiveTerminalNotification(event)
        default: break
        }
    }

    private func receiveCollaboration(_ event: RuntimeEvent) throws {
        if event.type.hasPrefix("room.") {
            try receiveRoom(event)
        } else {
            try receiveMission(event)
        }
    }

    private func receiveError(_ event: RuntimeEvent) {
        guard !event.type.hasPrefix("provider."), !event.type.hasPrefix("room.") else { return }
        people.absorb(error: event)
        let message = event.string("message") ?? String(localized: "That did not work.")
        let stage = account.stage
        account.absorb(error: event, message: message)
        guard !signInPresented || account.stage == stage else { return }
        errorMessage = message
    }

    private func receiveFriends(_ event: RuntimeEvent) throws {
        try people.receive(event)
        guard event.type == "friends.invite", let text = event.string("text"), let requested = inviteRequested,
              Date().timeIntervalSince(requested) < 15 else { return }
        inviteRequested = nil
        copyText(text)
        inviteCopies += 1
    }

    private func receiveTerminalNotification(_ event: RuntimeEvent) {
        if event.type == "term.bell", let id = event.string("sessionId") {
            noteAttention(id)
            return
        }
        guard event.type == "term.notification",
              let id = event.string("sessionId"),
              let incarnation = event.string("runtimeIncarnationId"),
              let current = session(id), current.kind == .local, current.incarnationId == incarnation
        else { return }
        terminalNotifications.post(title: event.string("title"), body: event.string("body"), session: current)
    }

    @discardableResult
    func openTerminalNotification(identifier: String) -> Bool {
        guard let id = terminalNotifications.sessionToOpen(identifier: identifier) else { return false }
        workbench.showsHistory = false
        workbench.launcher = nil
        workbench.detailsSessionId = nil
        workbench.sharingSessionId = nil
        activateSession(id)
        return true
    }

    private func receiveSystem(_ event: RuntimeEvent) throws {
        if event.type == "system.updateRequired" {
            updateRequired = true
            return
        }
        guard event.type == "system.ready" else { return }
        let version: UInt32 = try event.value("protocolVersion")
        guard version == RuntimeHandle.expectedProtocolVersion else {
            throw RuntimeError.invalidResponse(String(localized: "This copy of Kodosi is damaged. Install it again."))
        }
        appState = .ready
        refresh()
    }

    private func receiveAuth(_ event: RuntimeEvent) throws {
        let stage = account.stage
        try account.receiveAuth(event)
        switch event.type {
        case "auth.ready":
            if !account.localDeviceEnrolled, stage.isPending {
                signInPresented = true
            }
            commandSink.perform("session.list"); commandSink.perform("friends.refresh")
            commandSink.perform("devices.refresh"); commandSink.perform("mission.list")
            consumeDeepLink()
        case "auth.required":
            commandSink.perform("session.list")
        case "auth.deletion_pending":
            if let page = account.deletionPage {
                openExternalURL(page)
                account.closeDeletionPage()
            }
        case "auth.browser":
            if signInPresented, let url = account.signInPage, openedSignInPage != url {
                openedSignInPage = url
                openExternalURL(url)
            }
        case "auth.finalizing":
            if case .awaitingBrowser = stage {
                bringToFront()
            }
        default: break
        }
    }

    private func receiveSessions(_ event: RuntimeEvent) throws {
        guard event.type == "sessions.snapshot" else { return }
        let received: [RuntimeSession] = try event.value("sessions")
        guard Set(received.map(\.id)).count == received.count else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got a terminal list it cannot read."))
        }
        let previous = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        var createdIds: [String] = []
        sessions = received
        attention.formIntersection(received.map(\.id))
        freshTerminals.formIntersection(received.map(\.id))
        for value in received where previous[value.id].map(value.drawsAttention(after:)) ?? (value.canCall && value.waitState != nil) {
            noteAttention(value.id)
        }
        terminalNotifications.reconcile(sessions: received, accountEpoch: accountEpoch)
        alert(received, after: previous)
        for value in received {
            if let old = previous[value.id], old.incarnationId != value.incarnationId {
                terminalFocus.releaseSession(sessionId: value.id)
            }
            if let request = value.createRequestId, pendingCreations.remove(request) != nil {
                createdIds.append(value.id)
                freshTerminals.insert(value.id)
            }
            if let request = value.createRequestId, let room = roomCreations.removeValue(forKey: request) {
                freshTerminals.insert(value.id)
                activateSession(value.id, inRoom: room)
            }
        }
        for id in Set(previous.keys).subtracting(received.map(\.id)) {
            terminalFocus.releaseSession(sessionId: id)
        }
        reconcileTerminals()
        let restore = workbench.reconcile(received)
        for id in createdIds + restore {
            activateSession(id)
        }
        consumeDeepLink()
    }

    private func receiveMission(_ event: RuntimeEvent) throws {
        switch event.type {
        case "missions.snapshot":
            missions = try event.value("missions"); invitations = try event.value("invitations")
            let missionsTruncated = event.payload["missionsTruncated"] == .bool(true)
            missionListTruncated = missionsTruncated || event.payload["invitationsTruncated"] == .bool(true)
            if let selected = workbench.selectedMissionId {
                if let mission = missions.first(where: { $0.id == selected }) ?? (missionsTruncated ? missionDetail?.mission : nil) {
                    openMission(mission)
                } else if !missionsTruncated {
                    missionTask?.cancel(); missionTask = nil; missionRequestId = nil
                    workbench.selectedMissionId = nil; missionDetail = nil
                }
            }
        case "mission.snapshot":
            let mission: MissionEntry = try event.value("mission")
            if workbench.selectedMissionId == mission.id, event.string("requestId") == missionRequestId {
                missionDetail = try MissionDetail(mission: mission, members: event.value("members"))
            }
        case "mission.result":
            commandSink.perform("mission.list")
        default: break
        }
    }

    private func alert(_ received: [RuntimeSession], after before: [String: RuntimeSession]) {
        for value in received {
            guard let old = before[value.id], value.waitState != old.waitState else { continue }
            if let alert = value.alert, !isFront() {
                terminalNotifications.alert(alert, session: value)
            } else {
                terminalNotifications.withdrawAlert(value.id)
            }
        }
    }

    func noteAttention(_ id: String) {
        guard session(id) != nil, !isWatching(id) else { return }
        attention.insert(id)
    }

    func seen(_ id: String) {
        attention.remove(id)
        terminalNotifications.withdrawAlert(id)
    }

    func isWatching(_ id: String) -> Bool {
        guard isFront(), workbench.stagedSessionIds.contains(id) else { return false }
        switch workbench.section {
        case .sessions: return workbench.focusedSessionId == nil || workbench.focusedSessionId == id
        case .missions:
            guard let room = workbench.selectedMissionId, session(id)?.missionId == room else { return false }
            return roomView(room).canvas == 0 && roomView(room).selectedTerminal == id
        case .people, .settings: return false
        }
    }

    func reconcileTerminals() {
        terminalManager.reconcile(with: sessions)
    }
}
