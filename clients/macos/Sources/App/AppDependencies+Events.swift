import Foundation

extension AppDependencies {
    func consumeDeepLink() {
        guard let link = pendingDeepLink else { return }
        guard session(link.sessionId) != nil || (userId != nil && !signingIn) else { return }
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
            friends.removeAll(); incomingRequests.removeAll(); outgoingRequests.removeAll(); ownInvite = nil
            devices.removeAll(); deviceRequests.removeAll(); missions.removeAll(); invitations.removeAll()
            missionListTruncated = false
            missionDetail = nil
            rooms.removeAll(); roomIssues.removeAll(); roomViews.removeAll(); roomCreations.removeAll()
            selfDeviceId = nil; localDeviceEnrolled = false; identityMessage = nil
            activationTasks.values.forEach { $0.cancel() }; activationTasks.removeAll()
            pendingCreations.removeAll()
            missionTask?.cancel(); missionTask = nil; missionRequestId = nil
            accountKnown = true
            accountEpoch = event.accountEpoch
            userId = event.accountUserId
            commandSink.setAccount(userId: userId, epoch: accountEpoch)
            workbench.switchAccount(userId)
        }
        return event.accountEpoch == accountEpoch && event.accountUserId == userId
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
        case "devices": try receiveDevices(event)
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
        let message = event.string("message") ?? String(localized: "The action failed.")
        if event.type.hasPrefix("auth.") {
            signInStage = .failed(message: message, completed: signInStage.completedSteps)
            if signInPresented {
                return
            }
        }
        if event.type.hasPrefix("devices."), case .trustingDevice = signInStage {
            signInStage = .trustingDevice(.failed(message))
            if signInPresented {
                return
            }
        }
        errorMessage = message
    }

    private func receiveFriends(_ event: RuntimeEvent) throws {
        if event.type == "friends.invite" {
            ownInvite = event.string("text")
        } else {
            friends = try event.value("friends")
            incomingRequests = try event.value("incoming")
            outgoingRequests = try event.value("outgoing")
        }
    }

    private func receiveTerminalNotification(_ event: RuntimeEvent) {
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
        workbench.detailsSessionId = nil
        workbench.sharingSessionId = nil
        activateSession(id)
        return true
    }

    private func receiveSystem(_ event: RuntimeEvent) throws {
        guard event.type == "system.ready" else { return }
        let version: UInt32 = try event.value("protocolVersion")
        guard RuntimeHandle.isSupportedProtocolVersion(version) else {
            throw RuntimeError.invalidResponse(String(localized: "The runtime version does not match this app."))
        }
        appState = .ready
        refresh()
    }

    private func receiveReady(_ event: RuntimeEvent) throws {
        guard event.string("userId") == userId else {
            throw RuntimeError.invalidResponse(String(localized: "The runtime returned a mismatched account."))
        }
        let trusted = event.payload["enrolled"] != .bool(false)
        localDeviceEnrolled = trusted
        if trusted {
            identityMessage = nil
            signInStage = .signedIn
        } else {
            let wasSigningIn = signInStage.isPending
            if case .trustingDevice = signInStage {} else {
                signInStage = .trustingDevice(.choose)
            }
            if wasSigningIn {
                signInPresented = true
            }
        }
        commandSink.perform("session.list"); commandSink.perform("friends.refresh")
        commandSink.perform("devices.refresh"); commandSink.perform("mission.list")
        consumeDeepLink()
    }

    private func receiveAuth(_ event: RuntimeEvent) throws {
        switch event.type {
        case "auth.ready": try receiveReady(event)
        case "auth.required":
            if case .failed = signInStage {} else {
                signInStage = .idle
            }
            if event.string("reason") == "expired" {
                errorMessage = String(localized: "Your sign-in has ended. Sign in again.")
            }
            commandSink.perform("session.list")
        case "auth.device_code":
            let code = event.string("userCode") ?? ""
            let url = event.string("verificationUri").flatMap(ExternalAuthURL.parse)
            signInStage = .awaitingApproval(code: code, url: url)
            if signInPresented, let url, openedLoginCode != code {
                openedLoginCode = code
                openExternalURL(url)
            }
        case "auth.finalizing":
            signInStage = .finalizing
        default: break
        }
    }

    private func receiveSessions(_ event: RuntimeEvent) throws {
        guard event.type == "sessions.snapshot" else { return }
        let received: [RuntimeSession] = try event.value("sessions")
        guard Set(received.map(\.id)).count == received.count else {
            throw RuntimeError.invalidResponse(String(localized: "The runtime listed a session twice."))
        }
        let previous = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0.incarnationId) })
        var createdIds: [String] = []
        sessions = received
        terminalNotifications.reconcile(sessions: received, accountEpoch: accountEpoch)
        for value in received {
            if let old = previous[value.id], old != value.incarnationId {
                terminalFocus.releaseSession(sessionId: value.id)
            }
            if let request = value.createRequestId, pendingCreations.remove(request) != nil {
                createdIds.append(value.id)
            }
            if let request = value.createRequestId, let room = roomCreations.removeValue(forKey: request) {
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

    private func receiveDevices(_ event: RuntimeEvent) throws {
        switch event.type {
        case "devices.list":
            selfDeviceId = event.string("selfDeviceId")
            localDeviceEnrolled = try event.value("localDeviceEnrolled")
            identityMessage = localDeviceEnrolled ? nil : event.string("notice")
            devices = try event.value("devices")
        case "devices.link.snapshot": deviceRequests = try event.value("requests")
        case "devices.link.selfPending":
            signInStage = .trustingDevice(.pendingApproval(code: event.string("code") ?? ""))
        case "devices.link.selfResolved":
            if case .trustingDevice = signInStage, event.string("outcome") != "approved" {
                signInStage = .trustingDevice(.choose)
            }
            commandSink.perform("devices.refresh")
        case "devices.link.resolved": commandSink.perform("devices.refresh")
        default: break
        }
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

    func reconcileTerminals() {
        let visible = sessions.filter { $0.connectionState != .blocked }
        terminalManager.reconcileSessions(
            liveSessionIds: Set(visible.map(\.id)),
            runtimeIncarnations: Dictionary(uniqueKeysWithValues: visible.map { ($0.id, $0.incarnationId) }),
            resizeAuthority: Dictionary(uniqueKeysWithValues: visible.map { ($0.id, $0.canControl) })
        )
    }
}
