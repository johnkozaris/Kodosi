import AppKit
import Foundation
import KodosiBridge
import KodosiTerminalSessions
import Observation

@MainActor
@Observable
final class AppDependencies {
    enum AppState: Equatable { case launching, ready, error(String), hostOutdated(RuntimeStartFailure) }

    let storageBootstrap: RuntimeStorageBootstrap
    let commandSink: CommandSink
    let settings: DesktopSettings
    let workbench: WorkbenchState
    let launcher: LauncherState
    let account: AccountStore
    let people: PeopleStore
    let dosi: DosiStore
    var sessions: [RuntimeSession] = []
    var inviteRequested: Date?
    var inviteCopies = 0
    var missions: [MissionEntry] = []
    var invitations: [MissionInvitationEntry] = []
    var missionListTruncated = false
    var missionDetail: MissionDetail?
    var rooms: [String: RoomSnapshot] = [:]
    var roomIssues: [String: [RoomIssue]] = [:]
    var roomViews: [String: RoomViewState] = [:]
    var roomSeen: [String: UInt64] = [:]
    var reportedVisible: [String]?
    var restoreAsked = false
    var hostForeground = false
    var isFront: () -> Bool = { NSApp?.isActive == true }
    var freshTerminals: Set<String> = []
    var dosiTouches: [String: DosiTouch] = [:]
    var dosiTouched: Set<String> = []
    var signInPresented = false
    var openExternalURL: (URL) -> Void = { NSWorkspace.shared.open($0) }
    var bringToFront: () -> Void = { NSApp.activate() }
    var copyText: (String) -> Void = { text in
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    var openedSignInPage: URL?
    var errorMessage: String?
    let updater = AppUpdater()
    var updateRequired = false
    var commandLink: String?
    var appState: AppState = .launching
    var pendingDeepLink: DeepLinkDestination? {
        didSet { consumeDeepLink() }
    }

    var accountEpoch: UInt64 = 0
    var accountKnown = false
    @ObservationIgnored private var otherLaunchers: [String: LauncherState] = [:]
    @ObservationIgnored private var outdatedHostRestarted = false
    private var eventTask: Task<Void, Never>?
    var activationTasks: [String: Task<Void, Never>] = [:]
    var missionTask: Task<Void, Never>?
    var missionRequestId: String?
    private let inbox: RuntimeEventInbox
    let runtimeHandle: RuntimeHandle
    let terminalManager: TerminalSessionManager
    let terminalFocus: TerminalProcessFocusCoordinator
    let terminalNotifications: TerminalNotifications

    init(defaults: UserDefaults, storageBootstrap: RuntimeStorageBootstrap,
         notificationDriver: any DesktopNotificationDriving = SystemNotificationDriver())
    {
        terminalNotifications = TerminalNotifications(driver: notificationDriver)
        self.storageBootstrap = storageBootstrap
        let settings = DesktopSettings(defaults: defaults)
        self.settings = settings
        workbench = WorkbenchState(defaults: defaults)
        let commands = CommandSink()
        commandSink = commands
        account = AccountStore(commands: commands, browser: "system")
        people = PeopleStore(commands: commands)
        dosi = DosiStore(commands: commands)
        launcher = LauncherState(commands: commands)
        let inbox = RuntimeEventInbox()
        self.inbox = inbox
        let runtime = RuntimeHandle(inbox: inbox)
        runtimeHandle = runtime
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
    }

    func start() {
        guard eventTask == nil else { return }
        let currentInbox = inbox
        eventTask = Task { @MainActor [weak self] in
            while !Task.isCancelled, self?.runtimeHandle.isRunning == false {
                try? await Task.sleep(for: .milliseconds(10))
            }
            while !Task.isCancelled {
                guard let data = await currentInbox.next() else { break }
                guard !Task.isCancelled, let self else { return }
                do {
                    try receive(JSONDecoder().decode(RuntimeEvent.self, from: data))

                } catch {
                    failRuntime(String(localized: "Kodosi got an update it cannot read: \(error.localizedDescription)")); return
                }
            }
            if !Task.isCancelled, currentInbox.didOverflow {
                self?.failRuntime(String(localized: "Kodosi fell behind. Start it again."))
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
        Task { @MainActor [weak self] in
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
        missionTask?.cancel(); missionTask = nil; missionRequestId = nil
        activationTasks.values.forEach { $0.cancel() }
        activationTasks.removeAll()
        for session in sessions {
            terminalFocus.releaseSession(sessionId: session.id); terminalManager.releaseSession(for: session.id)
        }
        commandSink.cancelPending()
        commandSink.transport = nil
        otherLaunchers.removeAll()
        inbox.close()
        eventTask?.cancel()
        eventTask = nil
        runtimeHandle.stop(completion: completion)
    }

    private func failRuntime(_ message: String) {
        guard case .error = appState else {
            errorMessage = message
            present(.error(message))
            return
        }
    }

    func session(_ id: String) -> RuntimeSession? {
        sessions.first { $0.id == id }
    }

    var listedSessions: [RuntimeSession] {
        sessions.filter { !$0.hidden && $0.status != .closing }
    }

    func roomSessions(_ roomId: String) -> [RuntimeSession] {
        sessions.filter { $0.missionId == roomId && $0.status != .closing }
    }

    func refresh() {
        Task { await launcher.refresh(force: true) }
        commandSink.perform("session.list")
        if account.userId != nil {
            commandSink.perform("auth.refresh")
            commandSink.perform("friends.refresh")
            commandSink.perform("devices.refresh")
            commandSink.perform("mission.list")
        }
    }

    func activateSession(_ id: String, inRoom roomId: String? = nil) {
        guard workbench.stagedSessionIds.contains(id) || workbench.stagedSessionIds.count < WorkbenchState.maximumStagedSessions else {
            errorMessage = String(localized: "Six terminals are open. Minimize one to open another.")
            return
        }
        if let current = session(id) {
            guard current.canOpen else {
                errorMessage = String(localized: "This terminal is not available. Check that its computer is on.")
                return
            }
            workbench.showSession(id, inRoom: roomId)
            reportVisible()
            if let roomId {
                roomView(roomId).selectedTerminal = id
            }
            guard current.kind == .remote else { return }
        } else {
            guard account.userId != nil else { return }
        }
        guard activationTasks[id] == nil else { return }
        openRemote(id, inRoom: roomId)
    }

    func openStaged(_ id: String) {
        guard let current = session(id) else { return }
        if let replaced = current.restoredFrom {
            for state in roomViews.values where state.selectedTerminal == replaced {
                state.selectedTerminal = id
            }
        }
        guard current.kind == .remote, current.canOpen, activationTasks[id] == nil else { return }
        openRemote(id, inRoom: nil, shows: false)
    }

    private func openRemote(_ id: String, inRoom roomId: String?, shows: Bool = true) {
        let epoch = accountEpoch
        let section = workbench.section
        let selectedRoom = workbench.selectedMissionId
        activationTasks[id] = Task { @MainActor [weak self] in
            guard let self else { return }
            defer {
                if !Task.isCancelled {
                    activationTasks.removeValue(forKey: id)
                }
            }
            do {
                _ = try await commandSink.request("session.openRemote", ["sessionId": .string(id)])
                guard !Task.isCancelled, accountEpoch == epoch, session(id) != nil else { return }
                if shows, workbench.section == section, workbench.selectedMissionId == selectedRoom {
                    workbench.showSession(id, inRoom: roomId)
                }
                if let roomId {
                    roomView(roomId).selectedTerminal = id
                }
            } catch {
                guard !Task.isCancelled, accountEpoch == epoch else { return }
                errorMessage = error.localizedDescription
            }
        }
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

    func openLauncher(roomId: String? = nil) {
        let onHome = workbench.section == .sessions && !workbench.stagedSessionIds.contains { session($0) != nil }
        if onHome, roomId == nil {
            workbench.launcherFocus += 1
        } else {
            workbench.launcher = LauncherRequest(roomId: roomId)
        }
    }

    func openTerminal(in directory: String) {
        Task { @MainActor [weak self] in
            do {
                try await self?.createSession(name: generateSessionName(), directory: directory)
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

    func createSession(
        name: String, directory: String?, computer: String? = nil, roomId: String? = nil,
        resume: ProviderConversationIdentity? = nil, branch: String? = nil, command: String? = nil,
        agent: AgentStart? = nil, prompt: String? = nil,
    ) async throws {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name) else {
            throw RuntimeError.operation(String(localized: "Use a shorter name."))
        }
        var fields: [String: JSONValue] = ["name": .string(name)]
        if let roomId {
            roomView(roomId).canvas = 0
            fields["missionId"] = .string(roomId)
        }
        for (key, value) in [("workingDir", directory), ("computer", computer), ("command", command), ("prompt", prompt)] {
            if let value {
                fields[key] = .string(value)
            }
        }
        if let resume {
            fields["resume"] = .object(["provider": .string(resume.provider.rawValue), "nativeConversationId": .string(resume.nativeConversationId)])
        }
        if let branch {
            fields["branch"] = .string(branch)
            if let folder = settings.branchFolder {
                fields["worktrees"] = .string(folder)
            }
        }
        if let agent {
            fields["agent"] = agent.json
        }
        let created = try await commandSink.request("session.create", fields)
        if computer == nil {
            settings.lastWorkingDir = directory
            settings.save()
        }
        guard let id = created.string("sessionId") else { return }
        freshTerminals.insert(id)
        activateSession(id, inRoom: roomId)
    }

    func mutateSession(_ operation: String, session target: RuntimeSession, fields: [String: JSONValue] = [:]) async throws {
        try await commandSink.mutate(operation, session: target, current: session(target.id), fields: fields)
    }

    func openMission(_ mission: MissionEntry) {
        openMission(id: mission.id)
    }

    func openMission(id: String) {
        missionTask?.cancel()
        let requestId = UUIDv7.generate()
        missionRequestId = requestId
        workbench.selectedMissionId = id
        if missionDetail?.mission.id != id {
            missionDetail = nil
        }
        missionTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                let opened = try await commandSink.request("mission.open", ["missionId": .string(id), "requestId": .string(requestId)])
                guard !Task.isCancelled, missionRequestId == requestId, workbench.selectedMissionId == id else { return }
                missionDetail = try MissionDetail(mission: opened.value("mission"), members: opened.value("members"))
            } catch {
                guard !Task.isCancelled, missionRequestId == requestId else { return }
                workbench.selectedMissionId = nil
                missionDetail = nil
                errorMessage = error.localizedDescription
            }
        }
    }

    func perform(_ type: String, _ fields: [String: JSONValue] = [:]) {
        commandSink.perform(type, fields)
    }

    func copyInvite() {
        inviteRequested = Date()
        people.requestInvite()
    }
}
