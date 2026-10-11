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
    let commands: CommandSink
    let account: AccountStore
    let terminals: TerminalStore
    let rooms: RoomStore
    let people: PeopleStore

    @ObservationIgnored private let inbox: RuntimeEventInbox
    @ObservationIgnored private let runtime: RuntimeHandle
    @ObservationIgnored private var eventTask: Task<Void, Never>?
    @ObservationIgnored private var accountKnown = false
    @ObservationIgnored private(set) var accountEpoch: UInt64 = 0

    init() {
        let commands = CommandSink()
        let inbox = RuntimeEventInbox()
        self.commands = commands
        self.inbox = inbox
        let runtime = RuntimeHandle(inbox: inbox)
        self.runtime = runtime
        account = AccountStore(commands: commands, notices: notices)
        terminals = TerminalStore(commands: commands, notices: notices, runtime: runtime)
        rooms = RoomStore(commands: commands, notices: notices)
        people = PeopleStore(commands: commands, notices: notices)
        commands.onFailure = { [notices] message in notices.show(message) }
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

    func sceneChanged(to scenePhase: ScenePhase) {
        guard scenePhase == .active, phase == .ready else { return }
        refresh()
    }

    func refresh() {
        commands.perform("session.list")
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
        account.reset(userId: userId)
        terminals.reset(userId: userId)
        rooms.reset(userId: userId)
        people.reset(userId: userId)
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
        commands.receive(event)
        if event.type.hasSuffix(".error") {
            receiveError(event)
            return
        }
        switch event.type.prefix(while: { $0 != "." }) {
        case "system": try receiveSystem(event)
        case "auth": try receiveAuth(event)
        case "devices": try account.receiveDevices(event)
        case "sessions", "term": try terminals.receive(event)
        case "friends": try people.receive(event)
        case "mission", "missions", "room": try rooms.receive(event)
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
            commands.perform("session.list")
            commands.perform("friends.refresh")
            commands.perform("devices.refresh")
            commands.perform("mission.list")
        case "auth.required":
            commands.perform("session.list")
        default:
            break
        }
    }

    private func receiveError(_ event: RuntimeEvent) {
        guard !event.type.hasPrefix("provider."), !event.type.hasPrefix("room.") else { return }
        let message = event.string("message") ?? String(localized: "That did not work.")
        guard !account.absorb(error: event, message: message) else { return }
        notices.show(message)
    }
}
