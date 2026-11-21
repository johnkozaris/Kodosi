import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

private final class StandInMac {
    private struct Wake {
        let id: Int
        let at: TimeInterval
        let action: @MainActor () -> Void
    }

    var now: TimeInterval = 1000
    var lastInput: TimeInterval = 1000
    var active = false
    private var timers: [Wake] = []
    private var watchers: [Int: @MainActor () -> Void] = [:]
    private var made = 0

    var source: PresenceSource {
        PresenceSource(
            isActive: { [unowned self] in active },
            secondsSinceInput: { [unowned self] in now - lastInput },
            after: { [unowned self] seconds, action in
                made += 1
                let id = made
                timers.append(Wake(id: id, at: now + seconds, action: action))
                return { [unowned self] in timers.removeAll { $0.id == id } }
            },
            nextInput: { [unowned self] action in
                made += 1
                let id = made
                watchers[id] = action
                return { [unowned self] in watchers[id] = nil }
            },
        )
    }

    var wakes: [TimeInterval] {
        timers.map(\.at).sorted()
    }

    var watchesInput: Bool {
        !watchers.isEmpty
    }

    func pass(_ seconds: TimeInterval) {
        let end = now + seconds
        while let due = timers.min(by: { $0.at < $1.at }), due.at <= end {
            now = due.at
            timers.removeAll { $0.id == due.id }
            due.action()
        }
        now = end
    }

    func input() {
        lastInput = now
        let waiting = watchers.values
        watchers.removeAll()
        waiting.forEach { $0() }
    }
}

private final class Sent {
    var states: [Presence] = []
    var accepts = true
}

private func reporter(on mac: StandInMac) -> (PresenceReporter, Sent) {
    let sent = Sent()
    let reporter = PresenceReporter(source: mac.source) { state in
        guard sent.accepts else { return false }
        sent.states.append(state)
        return true
    }
    return (reporter, sent)
}

@Test func `the person is in Kodosi while it is in front and at the Mac while another app is`() {
    let mac = StandInMac()
    let (reporter, sent) = reporter(on: mac)
    mac.active = true
    reporter.connected(awaySeconds: 180)
    #expect(sent.states == [.kodosi])
    #expect(mac.wakes == [1180])
    mac.active = false
    reporter.update()
    #expect(sent.states == [.kodosi, .here])
    reporter.update()
    #expect(sent.states == [.kodosi, .here])
    #expect(mac.wakes == [1180])
}

@Test func `the Mac turns idle when the input stops for the away time and back at the next input`() {
    let mac = StandInMac()
    let (reporter, sent) = reporter(on: mac)
    reporter.connected(awaySeconds: 180)
    mac.pass(100)
    mac.input()
    mac.pass(80)
    #expect(sent.states == [.here])
    #expect(mac.wakes == [1280])
    mac.pass(100)
    #expect(sent.states == [.here, .idle])
    #expect(mac.wakes.isEmpty)
    #expect(mac.watchesInput)
    mac.pass(3600)
    #expect(sent.states == [.here, .idle])
    mac.active = true
    mac.input()
    #expect(sent.states == [.here, .idle, .kodosi])
    #expect(!mac.watchesInput)
    #expect(mac.wakes == [mac.now + 180])
}

@Test func `a locked screen, sleeping screens, a sleeping Mac or another user is away at once`() {
    let mac = StandInMac()
    let (reporter, sent) = reporter(on: mac)
    mac.active = true
    reporter.connected(awaySeconds: 180)
    for absence in [PresenceReporter.Absence.locked, .screensAsleep, .asleep, .switchedOut] {
        reporter.left(absence)
        #expect(sent.states.last == .away)
        #expect(mac.wakes.isEmpty)
        #expect(!mac.watchesInput)
        mac.input()
        reporter.returned(absence)
        #expect(sent.states.last == .kodosi)
    }
    #expect(sent.states == [.kodosi, .away, .kodosi, .away, .kodosi, .away, .kodosi, .away, .kodosi])

    reporter.left(.asleep)
    reporter.left(.screensAsleep)
    reporter.returned(.asleep)
    #expect(sent.states.last == .away)
    mac.pass(600)
    reporter.returned(.screensAsleep)
    #expect(sent.states.suffix(2) == [.away, .idle])
}

@Test func `a lock before the runtime is ready is away on its first report`() {
    let mac = StandInMac()
    let (reporter, sent) = reporter(on: mac)
    reporter.left(.locked)
    #expect(sent.states.isEmpty)
    reporter.connected(awaySeconds: 180)
    #expect(sent.states == [.away])
}

@Test func `each new runtime connection hears the state once, and a refused report goes out at the next change`() {
    let mac = StandInMac()
    let (reporter, sent) = reporter(on: mac)
    reporter.connected(awaySeconds: 180)
    reporter.update()
    #expect(sent.states == [.here])
    reporter.disconnected()
    #expect(mac.wakes.isEmpty)
    mac.active = true
    reporter.update()
    #expect(sent.states == [.here])
    reporter.connected(awaySeconds: 180)
    #expect(sent.states == [.here, .kodosi])
    reporter.connected(awaySeconds: 180)
    #expect(sent.states == [.here, .kodosi, .kodosi])

    sent.accepts = false
    mac.active = false
    reporter.update()
    sent.accepts = true
    reporter.update()
    #expect(sent.states == [.here, .kodosi, .kodosi, .here])
}

@Test func `app.presence goes to the runtime before the visible terminals, with the away time of system.ready`() throws {
    let mac = StandInMac()
    mac.active = true
    let defaults = try #require(EphemeralUserDefaults(prefix: "Presence"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, presenceSource: mac.source)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: [
        "protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion)), "awaySeconds": .uint(60),
    ]))
    let types = sent.compactMap { $0["type"]?.stringValue }
    let presence = try #require(types.firstIndex(of: "app.presence"))
    #expect(presence < (types.firstIndex(of: "session.visible") ?? types.count))
    #expect(sent[presence]["state"] == .string("kodosi"))
    #expect(mac.wakes == [mac.now + 60])
    sent.removeAll()
    mac.pass(60)
    #expect(sent.map { $0["state"] } == [.string("idle")])
    #expect(app.errorMessage == nil)
    try app.receive(runtimeEvent("app.error", epoch: 0, userId: nil, fields: [
        "requestId": .string(UUIDv7.generate()), "operation": .string("app.presence"), "message": .string("No"),
    ]))
    #expect(app.errorMessage == nil)
}
