import AppKit
import Foundation
import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions
import Observation

@Observable
final class AppDependencies {
    enum AppState: Equatable { case launching, ready, error(String), hostOutdated(RuntimeStartFailure) }

    let storageBootstrap: RuntimeStorageBootstrap
    let commandSink: CommandSink
    let settings: DesktopSettings
    let workbench: WorkbenchState
    let launcher: LauncherState
    let account: AccountStore
    let power: PowerStore
    let people: PeopleStore
    let dosi: DosiStore
    let canvas: CanvasStore
    let projects: ProjectStore
    let approvals: SharingApprovals
    let documentPages = CanvasDocumentPages()
    var sessions: [RuntimeSession] = []
    var inviteRequested: ContinuousClock.Instant?
    var inviteCopies = 0
    @ObservationIgnored var projectWorkspaces: [String: ProjectWorkspace] = [:]
    var reportedVisible: VisibleReport?
    var systemDark = false
    var themePreference = ThemePreference.system
    @ObservationIgnored var reportedLook: TerminalLook?
    @ObservationIgnored var appearanceWatch: NSKeyValueObservation?
    var restoreAsked = false
    var hostForeground = false
    var isFront: () -> Bool = { NSApp?.isActive == true }
    var windowShown: () -> Bool = { MainWindowLocator.titledWindow()?.occlusionState.contains(.visible) == true }
    var freshTerminals: Set<String> = []
    var dosiTouches: [String: DosiTouch] = [:]
    var dosiTouched: Set<String> = []
    var signInPresented = false
    var openExternalURL: (URL) -> Void = { NSWorkspace.shared.open($0) }
    var bringToFront: () -> Void = { MainWindowLocator.activate() }
    var copyText: (String) -> Void = { text in
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    var openedSignInPage: URL?
    var errorMessage: String?
    var doneMessage: String?
    let updater = AppUpdater()
    /// How many terminals would lose a running program if Kodosi restarted now.
    var restartStops = 0
    var updateRequired = false
    var updateNoticeDismissed = false
    var commandLink: String?
    var appState: AppState = .launching
    var pendingDeepLink: DeepLinkDestination? {
        didSet { consumeDeepLink() }
    }

    @ObservationIgnored var notified: (terminal: String, until: ContinuousClock.Instant)?
    @ObservationIgnored var openMainWindow: () -> Void = {}

    var accountEpoch: UInt64 = 0
    var accountKnown = false
    @ObservationIgnored private(set) lazy var dosiPlace = DosiPlace(
        dosi: dosi,
        terminal: { [weak self] in self?.terminal(named: $0) },
        project: { [weak self] in self?.project(named: $0) },
        sessions: { [weak self] in self?.listedSessions ?? [] },
        projects: { [weak self] in self?.projects.entries ?? [] },
        open: { [weak self] in self?.open($0) },
        answer: { [weak self] in self?.answerDosi($0, $1) },
        send: { [weak self] in self?.sendToDosi($0) },
        canSend: { [weak self] in self?.dosi.state != .unavailable },
        openSettings: { [weak self] in self?.workbench.showDosiSettings() },
    )
    @ObservationIgnored private var otherLaunchers: [String: LauncherState] = [:]
    @ObservationIgnored private var outdatedHostRestarted = false
    private var eventTask: Task<Void, Never>?
    var activationTasks: [String: Task<Void, Never>] = [:]
    private let inbox: RuntimeEventInbox
    let runtimeHandle: RuntimeHandle
    let terminalManager: TerminalSessionManager
    let terminalFocus: TerminalProcessFocusCoordinator
    let terminalNotifications: TerminalNotifications
    let presence: PresenceReporter
    let typeAhead = TerminalTypeAhead()
    var shortcutMonitor: Any?
    var shortcutRecorder: ((NSEvent) -> Bool)?

    init(defaults: UserDefaults, storageBootstrap: RuntimeStorageBootstrap,
         notificationDriver: any DesktopNotificationDriving = SystemNotificationDriver(),
         presenceSource: PresenceSource = .system)
    {
        terminalNotifications = TerminalNotifications(driver: notificationDriver)
        self.storageBootstrap = storageBootstrap
        let settings = DesktopSettings(defaults: defaults)
        self.settings = settings
        let commands = CommandSink()
        commandSink = commands
        presence = PresenceReporter(source: presenceSource) { [commands] state in
            CommandSink.didAcceptSynchronously(commands.send("app.presence", ["state": .string(state.rawValue)]))
        }
        let inbox = RuntimeEventInbox()
        self.inbox = inbox
        let runtime = RuntimeHandle(inbox: inbox)
        runtimeHandle = runtime
        let canvas = CanvasStore(commands: commands, videoPipes: runtime.videoPipes)
        self.canvas = canvas
        workbench = WorkbenchState(defaults: defaults, canvas: canvas)
        account = AccountStore(commands: commands, browser: "system")
        power = PowerStore(commands: commands)
        people = PeopleStore(commands: commands)
        dosi = DosiStore(commands: commands)
        projects = ProjectStore(commands: commands)
        approvals = SharingApprovals(commands: commands)
        launcher = LauncherState(commands: commands)
        let focus = TerminalProcessFocusCoordinator(commands: commands)
        terminalFocus = focus
        terminalManager = TerminalSessionManager(
            runtimeHandle: runtime, commandSink: commands, settings: settings,
            onFocusResult: { sessionId, incarnation, requestId, accepted in
                focus.applyResult(sessionId: sessionId, runtimeIncarnationId: incarnation,
                                  requestId: requestId, accepted: accepted)
            },
        )
        commandSink.onFailure = { [weak self] message in self?.errorMessage = message }
        account.notify = { [weak self] message in self?.errorMessage = message }
        projects.notify = { [weak self] message in self?.errorMessage = message }
        projects.isFront = { [weak self] in self?.isFront() == true }
        approvals.notify = { [weak self] message in self?.errorMessage = message }
    }

    func start() {
        guard eventTask == nil else { return }
        startShortcuts()
        let currentInbox = inbox
        eventTask = Task(name: "Runtime events") { [weak self] in
            while !Task.isCancelled, self?.runtimeHandle.isRunning == false {
                try? await Task.sleep(for: .milliseconds(10))
            }
            var faults = RuntimeEventFaults()
            while !Task.isCancelled {
                guard let item = await currentInbox.nextItem() else { break }
                guard !Task.isCancelled, let self else { return }
                switch item {
                case let .event(event):
                    do {
                        try receive(event)
                    } catch {
                        faults.unreadable(event.type, error)
                    }
                case let .unreadable(error):
                    faults.unreadable(nil, error)
                case let .behind(drop):
                    RuntimeEventFaults.behind(drop)
                    catchUp()
                }
            }
        }
        runtimeHandle.start(commandSink: commandSink) { [weak self] outcome in
            guard let self, eventTask != nil else { return }
            finishStart(outcome)
        }
    }

    private func finishStart(_ outcome: RuntimeHandle.StartOutcome) {
        switch outcome {
        case .started:
            if !storageBootstrap.isIsolated {
                terminalNotifications.activate()
            }
            appState = .ready
        case .incompatibleRuntime:
            failRuntime(String(localized: "This copy of Kodosi is damaged. Install it again."))
        case let .failed(failure):
            resolveStartFailure(failure)
        }
    }

    private func resolveStartFailure(_ failure: RuntimeStartFailure?) {
        guard let failure else {
            failRuntime(String(localized: "Terminals could not start."))
            return
        }
        if failure.code == .hostOutdated {
            present(.hostOutdated(failure))
            if failure.restartsWithoutAsking, !outdatedHostRestarted {
                outdatedHostRestarted = true
                restartOutdatedHost()
            }
        } else {
            failRuntime(failure.message)
        }
    }

    func restartOutdatedHost() {
        guard case let .hostOutdated(failure) = appState else { return }
        errorMessage = nil
        appState = .launching
        Task { [weak self] in
            let result = await HostStopResult.stopHost(force: true)
            guard let self else { return }
            switch result {
            case .accepted, .unreachable:
                retryStartup()
            case .refused:
                errorMessage = String(localized: "The earlier Kodosi did not stop.")
                present(.hostOutdated(failure))
            case .failed:
                errorMessage = String(localized: "Kodosi could not reach the earlier Kodosi.")
                present(.hostOutdated(failure))
            }
        }
    }

    private func present(_ state: AppState) {
        shutdownProcess()
        appState = state
    }

    func retryStartup() {
        errorMessage = nil
        appState = .launching
        shutdownProcess { [weak self] in
            guard let self else { return }
            inbox.reopen()
            accountKnown = false
            accountEpoch = 0
            reportedVisible = nil
            account.reset(userId: nil)
            commandSink.setAccount(userId: nil, epoch: 0)
            start()
        }
    }

    func shutdownProcess(completion: @escaping @MainActor () -> Void = {}) {
        signInPresented = false
        account.endSignIn()
        terminalNotifications.clear()
        presence.disconnected()
        reportedLook = nil
        activationTasks.values.forEach { $0.cancel() }
        activationTasks.removeAll()
        for session in sessions {
            terminalFocus.releaseSession(sessionId: session.id); terminalManager.releaseSession(for: session.id)
        }
        typeAhead.drop()
        approvals.reset()
        commandSink.cancelPending()
        commandSink.transport = nil
        otherLaunchers.removeAll()
        inbox.close()
        eventTask?.cancel()
        eventTask = nil
        runtimeHandle.stop(completion: completion)
    }

    /// Inbox overflow may have dropped replies or document frames, so reset waiters and resync.
    private func catchUp() {
        commandSink.abandonPending()
        canvas.renewPipes()
        approvals.reset()
        commandSink.send("app.resync")
    }

    func failRuntime(_ message: String) {
        if case .error = appState {
            return
        }
        present(.error(message))
    }

    func session(_ id: String) -> RuntimeSession? {
        sessions.first { $0.id == id }
    }

    var listedSessions: [RuntimeSession] {
        sessions.filter { !$0.hidden && $0.status != .closing }
    }

    func projectSessions(_ projectId: String) -> [RuntimeSession] {
        sessions.filter { $0.projectId == projectId && $0.status != .closing }
    }

    func refresh() {
        Task { await launcher.refresh(force: true) }
        commandSink.perform("session.list")
        if account.userId != nil {
            commandSink.perform("auth.refresh")
            commandSink.perform("friends.refresh")
            commandSink.perform("devices.refresh")
            commandSink.perform("project.list")
        }
    }

    @discardableResult
    func activateSession(_ id: String, inProject projectId: String? = nil) -> Bool {
        guard stageHasRoom(for: id) else {
            errorMessage = String(localized: "Six terminals are open. Minimize one to open another.")
            return false
        }
        if let current = session(id) {
            guard current.canOpen else {
                errorMessage = String(localized: "This terminal is not available. Check that its computer is on.")
                return false
            }
            makeRoom(for: id)
            workbench.showSession(id, inProject: projectId)
            if let projectId {
                projectWorkspace(projectId).selectedTerminal = id
            }
            reportVisible()
            if current.kind == .remote, activationTasks[id] == nil {
                openRemote(id, inProject: projectId)
            }
        } else if account.userId != nil, activationTasks[id] == nil {
            openRemote(id, inProject: projectId)
        }
        return workbench.stagedSessionIds.contains(id)
    }

    func dismissSession(_ id: String) {
        activationTasks.removeValue(forKey: id)?.cancel()
        workbench.dismissSession(id)
        terminalFocus.releaseSession(sessionId: id)
        terminalManager.releaseSession(for: id)
        if session(id)?.kind == .remote {
            commandSink.perform("session.disconnect", ["sessionId": .string(id)])
        }
    }

    var selectedLocalDirectory: String? {
        workbench.selectedSessionId.flatMap(session)?.localDirectory
    }

    func openLauncher(projectId: String? = nil) {
        let onHome = workbench.section == .sessions && !workbench.stagedSessionIds.contains { session($0) != nil }
        if onHome, projectId == nil {
            workbench.launcherFocus += 1
        } else {
            workbench.launcher = LauncherRequest(projectId: projectId)
        }
    }

    func openTerminal(in directory: String, computer: String? = nil, like: String? = nil) {
        Task { [weak self] in
            do {
                try await self?.createSession(directory: directory, computer: computer, like: like)
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func launcher(for computer: String?) -> LauncherState {
        guard let computer else { return launcher }
        if let known = otherLaunchers[computer] {
            return known
        }
        let made = LauncherState(commands: commandSink, computer: computer)
        otherLaunchers[computer] = made
        return made
    }

    @discardableResult
    func createSession(
        name: String? = nil, directory: String?, computer: String? = nil, projectId: String? = nil,
        resume: ProviderConversationIdentity? = nil, branch: String? = nil, command: String? = nil,
        agent: AgentStart? = nil, like: String? = nil, prompt: String? = nil,
    ) async throws -> String? {
        let name = name?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let name, !ProductInput.validName(name) {
            throw RuntimeError.operation(String(localized: "Use a shorter name."))
        }
        if let projectId {
            projectWorkspace(projectId).shownSection = 0
        }
        var fields = TerminalStart(
            name: name, directory: directory, computer: computer, projectId: projectId, command: command, agent: agent, like: like,
            prompt: prompt, branch: branch, worktrees: branch == nil ? nil : settings.branchFolder,
            size: startSize(computer: computer, projectId: projectId),
        ).fields
        if let resume {
            fields["resume"] = .object(["provider": .string(resume.provider.rawValue), "nativeConversationId": .string(resume.nativeConversationId)])
        }
        let created = try await commandSink.request("session.create", fields)
        if computer == nil {
            settings.lastWorkingDir = directory
            settings.save()
        }
        guard let id = created.string("sessionId") else { return nil }
        freshTerminals.insert(id)
        activateSession(id, inProject: projectId)
        return id
    }

    func mutateSession(_ operation: String, session target: RuntimeSession, fields: [String: JSONValue] = [:]) async throws {
        try await commandSink.mutate(operation, session: target, current: session(target.id), fields: fields)
    }

    /// Lets the terminal's agent go on with the task that a restart cut off.
    func continueTask(_ session: RuntimeSession) {
        Task {
            do { try await mutateSession("session.continue", session: session) } catch { errorMessage = error.localizedDescription }
        }
    }

    /// Shows what the terminal's branch folder changed on its canvas.
    func showChanges(_ session: RuntimeSession) {
        Task {
            do { try await mutateSession("session.changes", session: session) } catch { errorMessage = error.localizedDescription }
        }
    }

    func openProject(_ project: ProjectEntry) {
        openProject(id: project.id)
    }

    func openProject(id: String) {
        workbench.selectedProjectId = id
    }

    func perform(_ type: String, _ fields: [String: JSONValue] = [:]) {
        commandSink.perform(type, fields)
    }
}

extension AppDependencies {
    func openStaged(_ id: String) {
        guard let current = session(id) else { return }
        if let replaced = current.restoredFrom {
            for workspace in projectWorkspaces.values where workspace.selectedTerminal == replaced {
                workspace.selectedTerminal = id
            }
        }
        guard current.kind == .remote, current.canOpen, activationTasks[id] == nil else { return }
        openRemote(id, inProject: nil, asked: false)
    }

    private func openRemote(_ id: String, inProject projectId: String?, asked: Bool = true) {
        let epoch = accountEpoch
        let section = workbench.section
        let selectedProject = workbench.selectedProjectId
        activationTasks[id] = Task { [weak self] in
            guard let self else { return }
            defer {
                if !Task.isCancelled {
                    activationTasks.removeValue(forKey: id)
                }
            }
            do {
                _ = try await commandSink.request("session.openRemote", ["sessionId": .string(id)])
                guard !Task.isCancelled, accountEpoch == epoch, session(id) != nil else { return }
                if asked, workbench.section == section, workbench.selectedProjectId == selectedProject {
                    makeRoom(for: id)
                    workbench.showSession(id, inProject: projectId)
                }
                guard workbench.stagedSessionIds.contains(id) else {
                    commandSink.perform("session.disconnect", ["sessionId": .string(id)])
                    return
                }
                if let projectId {
                    projectWorkspace(projectId).selectedTerminal = id
                }
            } catch {
                guard session(id) == nil, !Task.isCancelled, accountEpoch == epoch else { return }
                errorMessage = error.localizedDescription
            }
        }
    }
}
