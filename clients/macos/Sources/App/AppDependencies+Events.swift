import AppKit
import Foundation
import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions

struct VisibleReport: Equatable {
    var sessionIds: [String]
    var watching: Bool
}

extension AppDependencies {
    func consumeDeepLink() {
        guard let link = pendingDeepLink, session(link.sessionId) != nil || (account.userId != nil && !account.stage.isPending) else { return }
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
            dosi.reset()
            projects.reset(userId: event.accountUserId); projectWorkspaces.removeAll()
            freshTerminals.removeAll(); reportedVisible = nil
            activationTasks.values.forEach { $0.cancel() }; activationTasks.removeAll()
            accountKnown = true
            accountEpoch = event.accountEpoch
            commandSink.setAccount(userId: account.userId, epoch: accountEpoch)
            workbench.switchAccount(account.userId)
        }
        return event.accountEpoch == accountEpoch && event.accountUserId == account.userId
    }

    func receive(_ event: RuntimeEvent) throws {
        guard acceptAccount(event) else { return }
        let delivery = commandSink.receive(event)
        if event.type.hasSuffix(".error") {
            receiveError(event, delivery: delivery)
            return
        }
        let family = event.type.prefix(while: { $0 != "." })
        switch family {
        case "system": try receiveSystem(event)
        case "auth": try receiveAuth(event)
        case "sessions": try receiveSessions(event)
        case "friends": try receiveFriends(event)
        case "devices": try account.receiveDevices(event)
        case "connection": try account.receiveConnection(event)
        case "project", "projects": try receiveProject(event)
        case "dosi": try receiveDosi(event)
        case "term", "session", "document": receiveTerminal(event)
        default: break
        }
    }

    private func receiveDosi(_ event: RuntimeEvent) throws {
        try dosi.receive(event)
        if event.type == "dosi.item" {
            try touch(by: event.value("item"))
        } else if event.type == "dosi.snapshot" {
            workbench.dosiOn = dosi.state != .off
        }
    }

    private func receiveError(_ event: RuntimeEvent, delivery: CommandSink.Delivery) {
        guard delivery == .app, !["provider.", "canvas.", "document.", "app."].contains(where: event.type.hasPrefix),
              event.string("operation") != "project.act" else { return }
        people.absorb(error: event)
        let message = event.string("message") ?? String(localized: "That did not work.")
        let stage = account.stage
        let inItsSheet = account.absorb(error: event, message: message)
            && ["devices.link.offer", "devices.link.check", "devices.link.approve"].contains(event.string("operation"))
        guard !inItsSheet, !signInPresented || account.stage == stage, !event.failedRefresh else { return }
        errorMessage = message
    }

    private func receiveFriends(_ event: RuntimeEvent) throws {
        try people.receive(event)
        guard event.type == "friends.result", event.string("operation") == "friends.invite",
              let text = event.string("text"), let requested = inviteRequested,
              requested.duration(to: .now) < .seconds(15) else { return }
        inviteRequested = nil
        copyText(text)
        inviteCopies += 1
    }

    private func receiveTerminal(_ event: RuntimeEvent) {
        if event.type.hasPrefix("document.") {
            canvas.receive(event)
            return
        }
        if event.type == "session.show", let id = event.string("sessionId") {
            showWindow()
            workbench.section = .sessions
            activateSession(id)
            return
        }
        if event.type == "session.picture" {
            terminalManager.receivePicture(event)
            return
        }
        guard let cue = event.string("cue").flatMap(TerminalNotifications.Cue.init) else { return }
        if event.type == "term.attention", let id = event.string("sessionId"), let current = session(id) {
            terminalNotifications.alert(current.alert, session: current, cue: cue)
            return
        }
        guard event.type == "term.notification",
              let id = event.string("sessionId"),
              let incarnation = event.string("runtimeIncarnationId"),
              let current = session(id), current.kind == .local, current.incarnationId == incarnation
        else { return }
        terminalNotifications.post(title: event.string("title"), body: event.string("body"), session: current, cue: cue)
    }

    func showWindow() {
        openMainWindow()
        bringToFront()
    }

    func openNotification(of terminal: String) {
        workbench.showsHistory = false
        workbench.showsPalette = false
        workbench.showsNewProject = false
        workbench.launcher = nil
        workbench.detailsSessionId = nil
        workbench.sharingSessionId = nil
        workbench.trusting = nil
        workbench.reporting = nil
        workbench.blocking = nil
        workbench.section = .sessions
        notified = (terminal, .now + .seconds(30))
        showNotifiedTerminal()
    }

    /// A notification that started the app arrives before the terminal list, and a terminal of
    /// the earlier run comes back under a new ID, so the notification waits a short time for it.
    private func showNotifiedTerminal() {
        guard let notified else { return }
        guard ContinuousClock.now < notified.until else {
            self.notified = nil
            return
        }
        guard let target = sessions.notificationTarget(notified.terminal) else { return }
        self.notified = nil
        activateSession(target.id)
        if canvasOpen(target.id) {
            canvas.use(target)
        }
    }

    private func receiveSystem(_ event: RuntimeEvent) throws {
        if event.type == "system.updateRequired" {
            updateRequired = true
            updater.checkInBackground()
            return
        }
        guard event.type == "system.ready" else { return }
        hostForeground = event.string("host") == "foreground"
        let version: UInt32 = try event.value("protocolVersion")
        guard version == RuntimeHandle.expectedProtocolVersion else {
            failRuntime(String(localized: "This copy of Kodosi is damaged. Install it again."))
            return
        }
        let awaySeconds: UInt64 = try event.value("awaySeconds")
        presence.connected(awaySeconds: TimeInterval(awaySeconds))
        appState = .ready
        restoreAsked = false
        refresh()
        reportedVisible = nil
        reportVisible()
    }

    private func askRestore() {
        guard !restoreAsked else { return }
        restoreAsked = true
        commandSink.perform("session.restore")
    }

    private func receiveAuth(_ event: RuntimeEvent) throws {
        let stage = account.stage
        try account.receiveAuth(event)
        switch event.type {
        case "auth.ready":
            askRestore()
            if !account.localDeviceEnrolled, stage.isPending {
                signInPresented = true
            }
            commandSink.perform("session.list"); commandSink.perform("friends.refresh")
            commandSink.perform("devices.refresh"); commandSink.perform("project.list")
            consumeDeepLink()
        case "auth.required":
            askRestore()
            commandSink.perform("session.list")
        case "auth.restoring":
            askRestore()
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
        guard received != sessions else { return }
        let previous = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        sessions = received
        canvas.observe(received)
        freshTerminals.formIntersection(received.map(\.id))
        terminalNotifications.reconcile(sessions: received, accountEpoch: accountEpoch)
        for value in received {
            if let old = previous[value.id], old.incarnationId != value.incarnationId {
                terminalFocus.releaseSession(sessionId: value.id)
            }
            if !value.attention || value.waiting == nil {
                terminalNotifications.withdrawAlert(value.id)
            }
        }
        for id in Set(previous.keys).subtracting(received.map(\.id)) {
            terminalFocus.releaseSession(sessionId: id)
        }
        for value in received where value.hidden && previous[value.id]?.hidden == false {
            dismissSession(value.id)
        }
        reconcileTerminals()
        let restored = workbench.reconcile(received)
        reportVisible()
        restored.forEach(openStaged)
        followCanvas()
        consumeDeepLink()
        showNotifiedTerminal()
    }

    private func receiveProject(_ event: RuntimeEvent) throws {
        switch event.type {
        case "project.show":
            guard let id = event.string("projectId") else { return }
            showWindow()
            workbench.section = .projects
            openProject(id: id)
        case "projects.snapshot":
            try projects.receive(event)
            if let selected = workbench.selectedProjectId, projects.departed.contains(selected) {
                workbench.selectedProjectId = nil
            }
        default:
            try projects.receive(event)
        }
    }

    func reportVisible() {
        let report = VisibleReport(sessionIds: shownSessionIds, watching: isFront())
        guard report != reportedVisible, commandSink.transport != nil else { return }
        reportedVisible = report
        commandSink.perform("session.visible", [
            "sessionIds": .array(report.sessionIds.map(JSONValue.string)), "watching": .bool(report.watching),
        ])
    }

    var shownSessionIds: [String] {
        guard windowShown() else { return [] }
        return sessions.map(\.id).filter(isShown).sorted()
    }

    var frontSessionId: String? {
        guard windowShown() else { return nil }
        let staged = workbench.stagedSessionIds
        switch workbench.section {
        case .sessions:
            if let focused = workbench.focusedSessionId {
                return staged.contains(focused) ? focused : nil
            }
            if let selected = workbench.selectedSessionId, staged.contains(selected) {
                return selected
            }
            return staged.count == 1 ? staged.first : nil
        case .projects:
            return staged.first(where: isShown)
        case .people, .settings:
            return nil
        }
    }

    func followCanvas() {
        guard let id = frontSessionId, let session = session(id) else { return }
        var open = canvasOpen(id)
        for news in canvas.news(session) {
            let opens = switch news {
            case .shown: open || settings.opensCanvas
            case .changed: !open && settings.opensCanvas && !canvas.isShut(id)
            }
            guard opens else { continue }
            // A show of the agent is no act of a person here, so the browser's page stays with
            // the device in use.
            canvas.reveal(news.shown, in: session)
            showCanvas(id)
            open = true
        }
    }

    private func canvasOpen(_ id: String) -> Bool {
        guard let session = session(id), canvas.showsSomething(session) else { return false }
        switch workbench.section {
        case .sessions: return workbench.canvasSessionId == id
        case .projects: return workbench.selectedProjectId.flatMap { projectWorkspaces[$0] }?.showsCanvas == true
        case .people, .settings: return false
        }
    }

    private func showCanvas(_ id: String) {
        switch workbench.section {
        case .sessions:
            workbench.showCanvas(id)
        case .projects:
            workbench.selectedProjectId.flatMap { projectWorkspaces[$0] }?.showsCanvas = true
        case .people, .settings:
            break
        }
    }

    private func isShown(_ id: String) -> Bool {
        guard workbench.stagedSessionIds.contains(id) else { return false }
        switch workbench.section {
        case .sessions: return workbench.focusedSessionId == nil || workbench.focusedSessionId == id
        case .projects:
            guard let project = workbench.selectedProjectId, session(id)?.projectId == project, let workspace = projectWorkspaces[project] else {
                return false
            }
            return workspace.shownSection == 0 && workspace.selectedTerminal == id
        case .people, .settings: return false
        }
    }

    func reconcileTerminals() {
        terminalManager.reconcile(with: sessions)
    }
}
