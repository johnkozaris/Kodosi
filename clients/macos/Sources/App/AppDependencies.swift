import AppKit
import Foundation
import Observation

@MainActor
@Observable
final class AppDependencies {
    enum AppState: Equatable { case launching, ready, error(String), hostConflict(RuntimeStartFailure) }

    let storageBootstrap: RuntimeStorageBootstrap
    let commandSink: CommandSink
    let settings: DesktopSettings
    let workbench: WorkbenchState
    var sessions: [RuntimeSession] = []
    var friends: [FriendEntry] = []
    var ownInvite: String?
    var incomingRequests: [FriendRequestEntry] = []
    var outgoingRequests: [FriendRequestEntry] = []
    var devices: [MyDeviceEntry] = []
    var deviceRequests: [DeviceLinkRequest] = []
    var missions: [MissionEntry] = []
    var invitations: [MissionInvitationEntry] = []
    var missionListTruncated = false
    var missionDetail: MissionDetail?
    var rooms: [String: RoomSnapshot] = [:]
    var roomIssues: [String: [RoomIssue]] = [:]
    var roomViews: [String: RoomViewState] = [:]
    var roomCreations: [String: String] = [:]
    var userId: String?
    var displayName: String?
    var attention: Set<String> = []
    var isFront: () -> Bool = { NSApp?.isActive == true }
    var freshTerminals: Set<String> = []
    var selfDeviceId: String?
    var localDeviceEnrolled = false
    var signInPresented = false
    var signInStage: SignInStage = .idle
    var signingIn: Bool {
        signInStage.isPending
    }

    var openExternalURL: (URL) -> Void = { NSWorkspace.shared.open($0) }
    var openedLoginCode: String?
    var identityMessage: String?
    var errorMessage: String?
    var appState: AppState = .launching
    var pendingDeepLink: DeepLinkDestination? {
        didSet { consumeDeepLink() }
    }

    var accountEpoch: UInt64 = 0
    var accountKnown = false
    private var hostTakeoverAttempted = false
    private var eventTask: Task<Void, Never>?
    var activationTasks: [String: Task<Void, Never>] = [:]
    var pendingCreations: Set<String> = []
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
        let inbox = RuntimeEventInbox()
        self.inbox = inbox
        let runtime = RuntimeHandle(inbox: inbox)
        runtimeHandle = runtime
        let focus = TerminalProcessFocusCoordinator(
            sendFocus: { lease, requestId, completion in
                completion(CommandSink.didAcceptSynchronously(commands.sendTerminalFocus(
                    sessionId: lease.sessionId, clientId: lease.subscriptionId,
                    subscriptionGeneration: lease.subscriptionGeneration,
                    requestId: requestId, expectedRuntimeIncarnationId: lease.runtimeIncarnationId
                )))
            },
            sendBlur: { authority, completion in
                completion(CommandSink.didAcceptSynchronously(commands.sendTerminalBlur(
                    sessionId: authority.lease.sessionId, clientId: authority.lease.subscriptionId,
                    subscriptionGeneration: authority.lease.subscriptionGeneration,
                    expectedRuntimeIncarnationId: authority.lease.runtimeIncarnationId
                )))
            }
        )
        terminalFocus = focus
        terminalManager = TerminalSessionManager(
            runtimeHandle: runtime, commandSink: commands, settings: settings,
            onFocusResult: { sessionId, incarnation, requestId, accepted in
                focus.applyResult(sessionId: sessionId, runtimeIncarnationId: incarnation,
                                  requestId: requestId, accepted: accepted)
            }
        )
        commandSink.onFailure = { [weak self] message in self?.errorMessage = message }
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
            hostTakeoverAttempted = false
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
        switch failure.resolution {
        case .takeOverIdleHost where !hostTakeoverAttempted:
            hostTakeoverAttempted = true
            Task { @MainActor [weak self] in
                let result = await HostStopResult.stopOtherHost(force: false)
                guard let self else { return }
                switch result {
                case .accepted, .unreachable: retryStartup()
                case .refused, .failed: present(.hostConflict(failure))
                }
            }
        case .takeOverIdleHost, .askToStopHost, .quitDuplicateApp:
            present(.hostConflict(failure))
        case .report:
            failRuntime(failure.message)
        }
    }

    func stopConflictingHost() {
        guard case let .hostConflict(failure) = appState else { return }
        errorMessage = nil
        appState = .launching
        Task { @MainActor [weak self] in
            let result = await HostStopResult.stopOtherHost(force: true)
            guard let self else { return }
            switch result {
            case .accepted, .unreachable:
                retryStartup()
            case .refused:
                errorMessage = String(localized: "The other Kodosi did not stop.")
                present(.hostConflict(failure))
            case .failed:
                errorMessage = String(localized: "Kodosi could not reach the other Kodosi.")
                present(.hostConflict(failure))
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
            userId = nil
            commandSink.setAccount(userId: nil, epoch: 0)
            start()
        }
    }

    func shutdownProcess(completion: @escaping @MainActor () -> Void = {}) {
        signInPresented = false
        signInStage = .idle
        terminalNotifications.clear()
        missionTask?.cancel(); missionTask = nil; missionRequestId = nil
        activationTasks.values.forEach { $0.cancel() }
        activationTasks.removeAll()
        for session in sessions {
            terminalFocus.releaseSession(sessionId: session.id); terminalManager.releaseSession(for: session.id)
        }
        commandSink.cancelPending()
        commandSink.transport = nil
        inbox.close()
        runtimeHandle.stop(completion: completion)
        eventTask?.cancel()
        eventTask = nil
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

    func refresh() {
        commandSink.perform("session.list")
        if userId != nil {
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
            seen(id)
            if let roomId {
                roomView(roomId).selectedTerminal = id
            }
            guard current.kind == .remote else { return }
        } else {
            guard userId != nil else { return }
        }
        guard activationTasks[id] == nil else { return }
        openRemote(id, inRoom: roomId)
    }

    private func openRemote(_ id: String, inRoom roomId: String?) {
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
                if workbench.section == section, workbench.selectedMissionId == selectedRoom {
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

    func newSession(directory: String? = nil) {
        let directory = directory ?? selectedLocalDirectory ?? settings.lastWorkingDir ?? storageBootstrap.defaultWorkingDirectory?.path
        Task { @MainActor in
            do {
                try await createSession(name: generateSessionName(), directory: directory)
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func createSession(name: String, directory: String?, resume: ProviderConversationIdentity? = nil, branch: String? = nil) async throws {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard ProductInput.validName(name) else {
            throw RuntimeError.operation(String(localized: "Use a shorter name."))
        }
        let requestId = UUIDv7.generate()
        pendingCreations.insert(requestId)
        var fields: [String: JSONValue] = ["requestId": .string(requestId), "name": .string(name)]
        if let directory {
            fields["workingDir"] = .string(directory)
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
        do {
            _ = try await commandSink.request("session.create", fields)
            settings.lastWorkingDir = directory
            settings.save()
        } catch {
            pendingCreations.remove(requestId)
            throw error
        }
    }

    func mutateSession(_ operation: String, session target: RuntimeSession, fields: [String: JSONValue] = [:]) async throws {
        guard let current = session(target.id), current.incarnationId == target.incarnationId else {
            throw RuntimeError.operation(String(localized: "This terminal changed. Try again."))
        }
        var fields = fields
        fields["sessionId"] = .string(current.id)
        fields["expectedRuntimeIncarnationId"] = .string(current.incarnationId)
        _ = try await commandSink.request(operation, fields)
    }

    func openMission(_ mission: MissionEntry) {
        openMission(id: mission.id)
    }

    func openMission(id: String) {
        missionTask?.cancel()
        let requestId = UUIDv7.generate()
        missionRequestId = requestId
        workbench.selectedMissionId = id
        missionDetail = nil
        missionTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                _ = try await commandSink.request("mission.open", ["missionId": .string(id), "requestId": .string(requestId)])
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
}
