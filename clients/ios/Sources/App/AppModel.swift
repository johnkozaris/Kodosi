import Foundation
import KodosiBridge
import Observation
import SwiftUI

@Observable
final class AppModel {
    enum Phase: Equatable {
        case launching
        case ready
        case failed(String)
    }

    private(set) var phase: Phase = .launching
    let notices = NoticeCenter()
    let lock = AppLock()
    let commands: CommandSink
    let account: AccountStore
    let terminals: TerminalStore
    let rooms: RoomStore
    let people: PeopleStore
    let dosi: DosiStore
    let push: PushNotifications
    /// The computer that the person picked to answer Dosi on this device.
    var dosiDevice: String?

    @ObservationIgnored private let inbox: RuntimeEventInbox
    @ObservationIgnored private let runtime: RuntimeHandle
    @ObservationIgnored private var eventTask: Task<Void, Never>?
    @ObservationIgnored private var accountKnown = false
    @ObservationIgnored private(set) var accountEpoch: UInt64 = 0
    @ObservationIgnored private var launchers: [String: LauncherState] = [:]
    @ObservationIgnored private var roomScreens: [String: RoomScreenState] = [:]
    @ObservationIgnored private var active = true

    init() {
        let commands = CommandSink()
        let inbox = RuntimeEventInbox()
        self.commands = commands
        self.inbox = inbox
        let runtime = RuntimeHandle(inbox: inbox)
        self.runtime = runtime
        account = AccountStore(commands: commands, browser: "app")
        terminals = TerminalStore(commands: commands, notices: notices, runtime: runtime)
        rooms = RoomStore(commands: commands)
        people = PeopleStore(commands: commands)
        dosi = DosiStore(commands: commands)
        push = PushNotifications(commands: commands)
        commands.onFailure = { [notices] message in notices.show(message) }
        account.notify = { [notices] message in notices.show(message) }
        rooms.notify = { [notices] message in notices.show(message) }
        rooms.isFront = { [weak self] in self?.inFront == true }
    }

    func start() {
        guard eventTask == nil else { return }
        do {
            try RuntimeEnvironment.install()
        } catch {
            phase = .failed(error.localizedDescription)
            return
        }
        let inbox = inbox
        let runtime = runtime
        eventTask = Task { [weak self] in
            while !Task.isCancelled, !runtime.isRunning {
                try? await Task.sleep(for: .milliseconds(10))
            }
            while !Task.isCancelled {
                guard let data = await inbox.next() else { break }
                guard !Task.isCancelled, let self else { return }
                do {
                    try receive(JSONDecoder().decode(RuntimeEvent.self, from: data))
                } catch {
                    fail(String(localized: "Kodosi got an update it cannot read: \(error.localizedDescription)"))
                    return
                }
            }
            if !Task.isCancelled, inbox.didOverflow {
                self?.fail(String(localized: "Kodosi fell behind. Start it again."))
            }
        }
        runtime.start(commandSink: commands) { [weak self] outcome in
            self?.finishStart(outcome)
        }
    }

    func retry() {
        phase = .launching
        shutdown { [weak self] in
            guard let self else { return }
            inbox.reopen()
            accountKnown = false
            accountEpoch = 0
            commands.setAccount(userId: nil, epoch: 0)
            resetStores(userId: nil)
            start()
        }
    }

    /// Whether the person sees what Kodosi shows: it is in front and not locked.
    private var inFront: Bool {
        active && !lock.locked
    }

    func sceneChanged(to scenePhase: ScenePhase) {
        lock.sceneChanged(to: scenePhase)
        active = scenePhase == .active
        rooms.markShownRead()
        guard scenePhase == .active, phase == .ready else { return }
        refresh()
    }

    /// How the phone shows a room, kept while the person moves around.
    func roomScreen(_ roomId: String) -> RoomScreenState {
        if let screen = roomScreens[roomId] {
            return screen
        }
        let screen = RoomScreenState()
        roomScreens[roomId] = screen
        return screen
    }

    /// The agents, choices and folders of one of the person's computers, for a new terminal there.
    func launcher(for computer: String) -> LauncherState {
        if let known = launchers[computer] {
            return known
        }
        let made = LauncherState(commands: commands, computer: computer)
        launchers[computer] = made
        return made
    }

    func refresh() {
        terminals.list()
        guard account.userId != nil else { return }
        commands.perform("auth.refresh")
        commands.perform("friends.refresh")
        commands.perform("devices.refresh")
        commands.perform("mission.list")
    }

    private func finishStart(_ outcome: RuntimeHandle.StartOutcome) {
        guard eventTask != nil else { return }
        switch outcome {
        case .started:
            break
        case .incompatibleRuntime:
            fail(String(localized: "This copy of Kodosi is damaged. Install it again."))
        case let .failed(failure):
            fail(failure?.message ?? String(localized: "Kodosi could not start."))
        }
    }

    private func fail(_ message: String) {
        guard phase != .failed(message) else { return }
        shutdown()
        phase = .failed(message)
    }

    private func shutdown(completion: @escaping @MainActor () -> Void = {}) {
        terminals.releaseAll()
        commands.cancelPending()
        commands.transport = nil
        inbox.close()
        runtime.stop(completion: completion)
        eventTask?.cancel()
        eventTask = nil
    }

    private func resetStores(userId: String?) {
        launchers.removeAll()
        roomScreens.removeAll()
        account.reset(userId: userId)
        terminals.reset(userId: userId)
        rooms.reset(userId: userId)
        people.reset(userId: userId)
        dosi.reset()
        dosiDevice = nil
    }

    private func acceptAccount(_ event: RuntimeEvent) -> Bool {
        let establishes = ["auth.ready", "auth.required", "system.ready"].contains(event.type)
        if !accountKnown || event.accountEpoch > accountEpoch {
            guard establishes else { return false }
            accountKnown = true
            accountEpoch = event.accountEpoch
            resetStores(userId: event.accountUserId)
            commands.setAccount(userId: event.accountUserId, epoch: event.accountEpoch)
        }
        return event.accountEpoch == accountEpoch && event.accountUserId == account.userId
    }

    func receive(_ event: RuntimeEvent) throws {
        guard acceptAccount(event) else { return }
        let delivery = commands.receive(event)
        if event.type.hasSuffix(".error") {
            receiveError(event, delivery: delivery)
            return
        }
        try dispatch(event)
    }

    private func dispatch(_ event: RuntimeEvent) throws {
        switch event.type.prefix(while: { $0 != "." }) {
        case "system": try receiveSystem(event)
        case "auth": try receiveAuth(event)
        case "devices": try account.receiveDevices(event)
        case "connection" where event.type == "connection.state": try account.receiveConnection(event)
        case "sessions", "session", "term", "document": try terminals.receive(event)
        case "friends": try people.receive(event)
        case "mission", "missions", "room": try rooms.receive(event)
        case "dosi": try dosi.receive(event)
        default: break
        }
    }

    private func receiveSystem(_ event: RuntimeEvent) throws {
        guard event.type == "system.ready" else { return }
        let version: UInt32 = try event.value("protocolVersion")
        guard version == RuntimeHandle.expectedProtocolVersion else {
            throw RuntimeError.invalidResponse(String(localized: "This copy of Kodosi is damaged. Install it again."))
        }
        phase = .ready
        refresh()
    }

    private func receiveAuth(_ event: RuntimeEvent) throws {
        try account.receiveAuth(event)
        switch event.type {
        case "auth.ready":
            terminals.list()
            commands.perform("friends.refresh")
            commands.perform("devices.refresh")
            commands.perform("mission.list")
        case "auth.required":
            terminals.list()
        default:
            break
        }
    }

    private func receiveError(_ event: RuntimeEvent, delivery: CommandSink.Delivery) {
        terminals.absorb(error: event)
        guard delivery == .app, !["provider.", "room.", "canvas.", "document."].contains(where: event.type.hasPrefix) else { return }
        let message = event.string("message") ?? String(localized: "That did not work.")
        people.absorb(error: event)
        guard !account.absorb(error: event, message: message), !event.failedRefresh else { return }
        notices.show(message)
    }
}
