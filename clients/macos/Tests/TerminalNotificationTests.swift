import Foundation
import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing
import UserNotifications

private final class NotificationDriver: DesktopNotificationDriving, @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [UNNotificationRequest] = []
    private var removed: [String] = []
    private var completions: [@Sendable (any Error?) -> Void] = []
    private var authorization: (@Sendable (UNAuthorizationStatus) -> Void)?
    let delaysDelivery: Bool
    let delaysAuthorization: Bool
    let existingIdentifiers: [String]

    init(delaysDelivery: Bool = false, delaysAuthorization: Bool = false, existingIdentifiers: [String] = []) {
        self.delaysDelivery = delaysDelivery
        self.delaysAuthorization = delaysAuthorization
        self.existingIdentifiers = existingIdentifiers
    }

    var posted: [UNNotificationRequest] {
        lock.withLock { requests }
    }

    var withdrawn: [String] {
        lock.withLock { removed }
    }

    func getAuthorizationStatus(_ completion: @escaping @Sendable (UNAuthorizationStatus) -> Void) {
        if delaysAuthorization {
            lock.withLock { authorization = completion }
        } else {
            completion(.authorized)
        }
    }

    func requestAuthorization() async throws -> Bool {
        true
    }

    func add(_ request: UNNotificationRequest, completion: @escaping @Sendable (any Error?) -> Void) {
        lock.withLock {
            requests.append(request)
            if delaysDelivery {
                completions.append(completion)
            }
        }
        if !delaysDelivery {
            completion(nil)
        }
    }

    func getIdentifiers(_ completion: @escaping @Sendable ([String]) -> Void) {
        completion(existingIdentifiers)
    }

    func remove(identifiers: [String]) {
        lock.withLock { removed.append(contentsOf: identifiers) }
    }

    func finishAuthorization() {
        let callback = lock.withLock { authorization }
        callback?(.authorized)
    }

    func finishDelivery() {
        let callbacks = lock.withLock {
            defer { completions.removeAll() }
            return completions
        }
        callbacks.forEach { $0(nil) }
    }
}

private func awaitNotification(_ condition: () -> Bool) async {
    for _ in 0 ..< 200 where !condition() {
        await Task.yield()
    }
    #expect(condition())
}

@Test func `a notification opens its terminal in front of sheets and Settings, and the terminal list once it is gone`() async throws {
    let driver = NotificationDriver()
    let defaults = try #require(EphemeralUserDefaults(prefix: "TerminalNotification"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: driver)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    let id = UUIDv7.generate()
    let incarnation = UUIDv7.generate()
    let entry: JSONValue = .object([
        "id": .string(id), "incarnationId": .string(incarnation), "kind": .string("local"),
        "name": .string("Quiet terminal"), "isOwner": .bool(true), "status": .string("running"),
        "attention": .bool(false), "hidden": .bool(false), "working": .bool(false), "connectionState": .string("local"), "sharedWith": .array([]),
    ])
    try app.receive(runtimeReady())
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([entry])]))
    try app.receive(runtimeEvent("term.notification", fields: [
        "sessionId": .string(id), "runtimeIncarnationId": .string(incarnation), "body": .string("Ready"), "cue": .string("banner"),
    ]))
    await awaitNotification { driver.posted.count == 1 }
    let terminal = try #require(driver.posted.first?.content.userInfo[TerminalNotifications.terminalKey] as? String)
    #expect(app.workbench.stagedSessionIds.isEmpty)
    app.workbench.section = .settings
    app.workbench.showsHistory = true
    app.workbench.showsPalette = true
    app.workbench.launcher = LauncherRequest()
    app.workbench.detailsSessionId = id
    app.workbench.sharingSessionId = id
    app.openNotification(of: terminal)
    #expect(!app.workbench.showsHistory)
    #expect(!app.workbench.showsPalette)
    #expect(app.workbench.launcher == nil)
    #expect(app.workbench.detailsSessionId == nil)
    #expect(app.workbench.sharingSessionId == nil)
    #expect(app.workbench.section == .sessions)
    #expect(app.workbench.selectedSessionId == id)
    #expect(app.workbench.stagedSessionIds == [id])

    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([])]))
    app.workbench.section = .settings
    app.openNotification(of: terminal)
    #expect(app.workbench.section == .sessions)
    #expect(app.workbench.stagedSessionIds.isEmpty)
    #expect(app.errorMessage == nil)
}

@Test func `a notification from before Quit opens the terminal that comes back in its place`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "TerminalNotificationRestart"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: NotificationDriver())
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    let ended = UUIDv7.generate()
    let again = UUIDv7.generate()
    app.openNotification(of: ended)
    try app.receive(runtimeReady())
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([])]))
    #expect(app.workbench.stagedSessionIds.isEmpty)
    let restored: JSONValue = .object([
        "id": .string(again), "incarnationId": .string(again), "kind": .string("local"), "restoredFrom": .string(ended),
        "name": .string("Review"), "isOwner": .bool(true), "status": .string("running"),
        "attention": .bool(false), "hidden": .bool(false), "working": .bool(false), "connectionState": .string("local"), "sharedWith": .array([]),
    ])
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([restored])]))
    #expect(app.workbench.stagedSessionIds == [again])
}

@Test func `the runtime's call posts its terminal also while Kodosi is in front, as loud as its cue says`() async throws {
    let driver = NotificationDriver()
    let defaults = try #require(EphemeralUserDefaults(prefix: "TerminalAlert"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: driver)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    app.isFront = { true }
    let own = UUIDv7.generate()
    let studio = UUIDv7.generate()
    func entry(_ id: String, _ name: String, attention: Bool, _ status: [String: JSONValue]) -> JSONValue {
        .object([
            "id": .string(id), "incarnationId": .string(id), "kind": .string(id == own ? "local" : "remote"),
            "name": .string(name), "isOwner": .bool(true), "status": .string("running"),
            "attention": .bool(attention), "hidden": .bool(false), "working": .bool(false),
            "connectionState": .string(id == own ? "local" : "connected"), "sharedWith": .array([]),
            "hostDeviceId": .string(id == own ? "this-mac" : "studio-mac"), "hostName": .string(id == own ? "Laptop" : "Studio"),
            "programStatus": .object(status), "waiting": status["state"] ?? .null,
        ])
    }
    func receive(_ status: [String: JSONValue], attention: Bool) throws {
        try app.receive(runtimeEvent("sessions.snapshot", fields: [
            "sessions": .array([entry(own, "Review", attention: attention, status), entry(studio, "Build", attention: attention, status)]),
        ]))
    }
    func call(_ id: String, cue: String) throws {
        try app.receive(runtimeEvent("term.attention", fields: ["sessionId": .string(id), "cue": .string(cue)]))
    }
    func settle() async {
        for _ in 0 ..< 20 {
            await Task.yield()
        }
    }
    let waits: [String: JSONValue] = ["state": .string("blocked"), "kind": .string("permission"), "message": .string("Allow the command?")]
    try app.receive(runtimeReady())
    try receive(waits, attention: true)
    try call(own, cue: "sound")
    try call(studio, cue: "list")
    await awaitNotification { driver.posted.count == 2 }
    let loud = try #require(driver.posted.first { $0.content.title == "Review" })
    #expect(loud.content.body == "Allow the command?")
    #expect(loud.content.sound != nil)
    #expect(loud.content.interruptionLevel == .active)
    #expect(loud.content.subtitle.isEmpty)
    let quiet = try #require(driver.posted.first { $0.content.title == "Build" })
    #expect(quiet.content.sound == nil)
    #expect(quiet.content.interruptionLevel == .passive)
    #expect(quiet.content.subtitle == "Studio")
    #expect(quiet.content.threadIdentifier == "studio-mac")
    #expect(loud.content.threadIdentifier == "this-mac")

    await settle()
    try receive(waits, attention: false)
    #expect(Set(driver.withdrawn) == [loud.identifier, quiet.identifier])

    try receive(["state": .string("done")], attention: true)
    try call(own, cue: "banner")
    await awaitNotification { driver.posted.count == 3 }
    let again = try #require(driver.posted.last)
    #expect(again.identifier == loud.identifier)
    #expect(again.content.body == "Done")
    #expect(again.content.sound == nil)
    #expect(again.content.interruptionLevel == .active)
}

@Test func `a program's own notification sounds only when its cue says so`() async throws {
    let driver = NotificationDriver()
    let defaults = try #require(EphemeralUserDefaults(prefix: "ProgramNotification"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: driver)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    let id = UUIDv7.generate()
    let incarnation = UUIDv7.generate()
    try app.receive(runtimeReady())
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([.object([
        "id": .string(id), "incarnationId": .string(incarnation), "kind": .string("local"),
        "name": .string("Tests"), "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false),
        "hidden": .bool(false), "working": .bool(false), "connectionState": .string("local"), "sharedWith": .array([]),
    ])])]))
    func notify(_ body: String, cue: String) throws {
        try app.receive(runtimeEvent("term.notification", fields: [
            "sessionId": .string(id), "runtimeIncarnationId": .string(incarnation), "body": .string(body), "cue": .string(cue),
        ]))
    }
    try notify("passed", cue: "banner")
    await awaitNotification { driver.posted.count == 1 }
    #expect(driver.posted.last?.content.sound == nil)
    for _ in 0 ..< 20 {
        await Task.yield()
    }
    try notify("failed", cue: "sound")
    await awaitNotification { driver.posted.count == 2 }
    #expect(driver.posted.last?.content.sound != nil)
    #expect(driver.posted.last?.identifier == driver.posted.first?.identifier)
}

@Test func `a program's own notification leaves when the mark of its terminal ends`() async throws {
    let driver = NotificationDriver()
    let defaults = try #require(EphemeralUserDefaults(prefix: "ProgramNotificationMark"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: driver)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    let id = UUIDv7.generate()
    let incarnation = UUIDv7.generate()
    func list(_ name: String, marked: Bool) throws {
        try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([.object([
            "id": .string(id), "incarnationId": .string(incarnation), "kind": .string("local"),
            "name": .string(name), "isOwner": .bool(true), "status": .string("running"), "attention": .bool(marked),
            "hidden": .bool(false), "working": .bool(false), "connectionState": .string("local"), "sharedWith": .array([]),
        ])])]))
    }
    try app.receive(runtimeReady())
    try list("Backup", marked: true)
    try app.receive(runtimeEvent("term.notification", fields: [
        "sessionId": .string(id), "runtimeIncarnationId": .string(incarnation), "body": .string("Finished"), "cue": .string("banner"),
    ]))
    await awaitNotification { driver.posted.count == 1 }
    let posted = try #require(driver.posted.first)
    try list("Nightly backup", marked: true)
    #expect(driver.withdrawn.isEmpty)
    try list("Nightly backup", marked: false)
    #expect(driver.withdrawn == [posted.identifier])
}

@Test func `kodosi show opens the window that the person closed`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "ShowTerminal"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: NotificationDriver())
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    var shown: [String] = []
    app.openMainWindow = { shown.append("window") }
    app.bringToFront = { shown.append("front") }
    let session = try runtimeSession(kind: "local", connection: "local")
    try app.receive(runtimeReady())
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([.object([
        "id": .string(session.id), "incarnationId": .string(session.incarnationId), "kind": .string("local"),
        "name": .string("Review"), "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false),
        "hidden": .bool(false), "working": .bool(false), "connectionState": .string("local"), "sharedWith": .array([]),
    ])])]))
    try app.receive(runtimeEvent("session.show", fields: ["sessionId": .string(session.id)]))
    #expect(shown == ["window", "front"])
    #expect(app.workbench.stagedSessionIds == [session.id])
    try app.receive(runtimeEvent("project.show", fields: ["projectId": .string(UUIDv7.generate())]))
    #expect(shown == ["window", "front", "window", "front"])
    #expect(app.workbench.section == .projects)
}

@Test func `terminal notification late delivery cannot outlive its session`() async throws {
    let driver = NotificationDriver(delaysDelivery: true)
    let notifications = TerminalNotifications(driver: driver)
    let session = try runtimeSession(kind: "local", connection: "local")
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    notifications.post(title: nil, body: "done", session: session, cue: .banner)
    await awaitNotification { driver.posted.count == 1 }
    let identifier = try #require(driver.posted.first?.identifier)
    notifications.reconcile(sessions: [], accountEpoch: 1)
    #expect(driver.withdrawn == [identifier])
    driver.finishDelivery()
    await awaitNotification { driver.withdrawn.count == 2 }
}

@Test func `terminal notification pending authorization cannot cross restart or account`() async throws {
    let driver = NotificationDriver(delaysAuthorization: true)
    let notifications = TerminalNotifications(driver: driver)
    let session = try runtimeSession(kind: "local", connection: "local")
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    notifications.post(title: nil, body: "old", session: session, cue: .banner)
    notifications.clear()
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    driver.finishAuthorization()
    for _ in 0 ..< 100 {
        await Task.yield()
    }
    #expect(driver.posted.isEmpty)
    notifications.post(title: nil, body: "another account", session: session, cue: .banner)
    notifications.reconcile(sessions: [session], accountEpoch: 2)
    driver.finishAuthorization()
    for _ in 0 ..< 100 {
        await Task.yield()
    }
    #expect(driver.posted.isEmpty)
}

@Test func `terminal notification coalesces and bounds inflight work`() async throws {
    let driver = NotificationDriver(delaysDelivery: true)
    let notifications = TerminalNotifications(driver: driver)
    let sessions = try (0 ..< 40).map { _ in try runtimeSession(UUIDv7.generate(), kind: "local", connection: "local") }
    notifications.reconcile(sessions: sessions, accountEpoch: 1)
    for session in sessions {
        notifications.post(title: String(repeating: "x", count: 2000), body: "ready", session: session, cue: .banner)
        notifications.post(title: nil, body: "duplicate", session: session, cue: .banner)
    }
    await awaitNotification { driver.posted.count == 32 }
    #expect(Set(driver.posted.map(\.identifier)).count == 32)
    #expect(driver.posted.allSatisfy { $0.content.title.count == 1024 })
    notifications.clear()
    driver.finishDelivery()
}

@Test func `terminal notification withdraws on replacement and on another account`() async throws {
    let driver = NotificationDriver()
    let notifications = TerminalNotifications(driver: driver)
    let session = try runtimeSession(kind: "local", connection: "local")
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    notifications.post(title: nil, body: "ready", session: session, cue: .banner)
    await awaitNotification { driver.posted.count == 1 }
    let old = try #require(driver.posted.first?.identifier)
    let replacement = try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode([
        "id": JSONValue.string(session.id), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("Replacement"), "isOwner": .bool(true),
        "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
        "connectionState": .string("local"), "sharedWith": .array([]),
    ]))
    notifications.reconcile(sessions: [replacement], accountEpoch: 1)
    #expect(driver.withdrawn.contains(old))
    notifications.post(title: nil, body: "new", session: replacement, cue: .banner)
    await awaitNotification { driver.posted.count == 2 }
    let current = try #require(driver.posted.last?.identifier)
    notifications.reconcile(sessions: [replacement], accountEpoch: 2)
    #expect(driver.withdrawn.contains(current))
}

@Test func `terminal notification startup removes only older terminal notifications`() async {
    let driver = NotificationDriver(existingIdentifiers: ["terminal-old-runtime", "unrelated-feature"])
    let notifications = TerminalNotifications(driver: driver)
    notifications.activate()
    await awaitNotification { !driver.withdrawn.isEmpty }
    #expect(driver.withdrawn == ["terminal-old-runtime"])
}
