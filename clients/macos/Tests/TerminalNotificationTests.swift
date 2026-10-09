import Foundation
@testable import KodosiDesktop
import Testing
import UserNotifications

private final class NotificationDriver: DesktopNotificationDriving, @unchecked Sendable {
    private let lock = NSLock()
    private var requests: [UNNotificationRequest] = []
    private var removed: [String] = []
    private var completions: [@Sendable (Error?) -> Void] = []
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

    func add(_ request: UNNotificationRequest, completion: @escaping @Sendable (Error?) -> Void) {
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

@MainActor
private func awaitNotification(_ condition: () -> Bool) async {
    for _ in 0 ..< 200 where !condition() {
        await Task.yield()
    }
    #expect(condition())
}

@Test @MainActor func terminalNotificationOpensCurrentMinimizedSessionAndWithdrawsOnExit() async throws {
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
        "connectionState": .string("local"), "sharedWith": .array([]),
    ])
    try app.receive(runtimeEvent("system.ready", fields: ["protocolVersion": .int(52)]))
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([entry])]))
    try app.receive(runtimeEvent("term.notification", fields: [
        "sessionId": .string(id), "runtimeIncarnationId": .string(incarnation), "body": .string("Ready"),
    ]))
    await awaitNotification { driver.posted.count == 1 }
    let request = try #require(driver.posted.first)
    #expect(app.workbench.stagedSessionIds.isEmpty)
    app.workbench.showsHistory = true
    app.workbench.detailsSessionId = id
    app.workbench.sharingSessionId = id
    #expect(app.openTerminalNotification(identifier: request.identifier))
    #expect(!app.workbench.showsHistory)
    #expect(app.workbench.detailsSessionId == nil)
    #expect(app.workbench.sharingSessionId == nil)
    #expect(app.workbench.selectedSessionId == id)
    #expect(app.workbench.stagedSessionIds == [id])
    #expect(driver.withdrawn.contains(request.identifier))
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([])]))
    #expect(!app.openTerminalNotification(identifier: request.identifier))
}

@Test @MainActor func aTerminalThatNeedsItsOwnerSendsOneAlertWhileTheAppIsNotInFront() async throws {
    let driver = NotificationDriver()
    let defaults = try #require(EphemeralUserDefaults(prefix: "TerminalAlert"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap, notificationDriver: driver)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    app.isFront = { false }
    let own = UUIDv7.generate()
    let shared = UUIDv7.generate()
    func entry(_ id: String, owner: Bool, _ status: [String: JSONValue]) -> JSONValue {
        .object([
            "id": .string(id), "incarnationId": .string(id), "kind": .string(owner ? "local" : "remote"),
            "name": .string(owner ? "Review" : "Shared"), "isOwner": .bool(owner), "status": .string("running"),
            "connectionState": .string(owner ? "local" : "connected"), "sharedWith": .array([]), "programStatus": .object(status),
        ])
    }
    func receive(_ status: [String: JSONValue]) throws {
        try app.receive(runtimeEvent("sessions.snapshot", fields: [
            "sessions": .array([entry(own, owner: true, status), entry(shared, owner: false, status)]),
        ]))
    }
    let waits: [String: JSONValue] = ["state": .string("blocked"), "kind": .string("permission"), "message": .string("Allow the command?")]
    try app.receive(runtimeEvent("system.ready", fields: ["protocolVersion": .int(52)]))
    try receive(waits)
    #expect(driver.posted.isEmpty)
    try receive(["state": .string("working")])
    try receive(waits)
    await awaitNotification { driver.posted.count == 1 }
    let first = try #require(driver.posted.first)
    #expect(first.content.title == "Review")
    #expect(first.content.body == "Allow the command?")
    for _ in 0 ..< 20 {
        await Task.yield()
    }

    try receive(["state": .string("working")])
    #expect(driver.withdrawn == [first.identifier])
    try receive(["state": .string("done")])
    await awaitNotification { driver.posted.count == 2 }
    #expect(driver.posted.last?.content.body == "Done")
    for _ in 0 ..< 20 {
        await Task.yield()
    }
    app.seen(own)
    #expect(driver.withdrawn.count == 2)

    app.isFront = { true }
    try receive(["state": .string("working")])
    try receive(["state": .string("error")])
    #expect(driver.posted.count == 2)
    #expect(app.attention == [own, shared])
}

@Test @MainActor func terminalNotificationLateDeliveryCannotOutliveItsSession() async throws {
    let driver = NotificationDriver(delaysDelivery: true)
    let notifications = TerminalNotifications(driver: driver)
    let session = try runtimeSession(kind: "local", connection: "local")
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    notifications.post(title: nil, body: "done", session: session)
    await awaitNotification { driver.posted.count == 1 }
    let identifier = try #require(driver.posted.first?.identifier)
    notifications.reconcile(sessions: [], accountEpoch: 1)
    #expect(driver.withdrawn == [identifier])
    driver.finishDelivery()
    await awaitNotification { driver.withdrawn.count == 2 }
    #expect(notifications.sessionToOpen(identifier: identifier) == nil)
}

@Test @MainActor func terminalNotificationPendingAuthorizationCannotCrossRestartOrAccount() async throws {
    let driver = NotificationDriver(delaysAuthorization: true)
    let notifications = TerminalNotifications(driver: driver)
    let session = try runtimeSession(kind: "local", connection: "local")
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    notifications.post(title: nil, body: "old", session: session)
    notifications.clear()
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    driver.finishAuthorization()
    for _ in 0 ..< 100 {
        await Task.yield()
    }
    #expect(driver.posted.isEmpty)
    notifications.post(title: nil, body: "another account", session: session)
    notifications.reconcile(sessions: [session], accountEpoch: 2)
    driver.finishAuthorization()
    for _ in 0 ..< 100 {
        await Task.yield()
    }
    #expect(driver.posted.isEmpty)
}

@Test @MainActor func terminalNotificationCoalescesAndBoundsInflightWork() async throws {
    let driver = NotificationDriver(delaysDelivery: true)
    let notifications = TerminalNotifications(driver: driver)
    let sessions = try (0 ..< 40).map { _ in try runtimeSession(UUIDv7.generate(), kind: "local", connection: "local") }
    notifications.reconcile(sessions: sessions, accountEpoch: 1)
    for session in sessions {
        notifications.post(title: String(repeating: "x", count: 2000), body: "ready", session: session)
        notifications.post(title: nil, body: "duplicate", session: session)
    }
    await awaitNotification { driver.posted.count == 32 }
    #expect(Set(driver.posted.map(\.identifier)).count == 32)
    #expect(driver.posted.allSatisfy { $0.content.title.count == 1024 })
    notifications.clear()
    driver.finishDelivery()
}

@Test @MainActor func terminalNotificationRejectsReplacementAndPriorAccountActions() async throws {
    let driver = NotificationDriver()
    let notifications = TerminalNotifications(driver: driver)
    let session = try runtimeSession(kind: "local", connection: "local")
    notifications.reconcile(sessions: [session], accountEpoch: 1)
    notifications.post(title: nil, body: "ready", session: session)
    await awaitNotification { driver.posted.count == 1 }
    let old = try #require(driver.posted.first?.identifier)
    let replacement = try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode([
        "id": JSONValue.string(session.id), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("Replacement"), "isOwner": .bool(true),
        "status": .string("running"), "connectionState": .string("local"), "sharedWith": .array([]),
    ]))
    notifications.reconcile(sessions: [replacement], accountEpoch: 1)
    #expect(notifications.sessionToOpen(identifier: old) == nil)
    #expect(driver.withdrawn.contains(old))
    notifications.post(title: nil, body: "new", session: replacement)
    await awaitNotification { driver.posted.count == 2 }
    let current = try #require(driver.posted.last?.identifier)
    notifications.reconcile(sessions: [replacement], accountEpoch: 2)
    #expect(notifications.sessionToOpen(identifier: current) == nil)
    #expect(driver.withdrawn.contains(current))
}

@Test @MainActor func terminalNotificationStartupRemovesOnlyOlderTerminalNotifications() async {
    let driver = NotificationDriver(existingIdentifiers: ["terminal-old-runtime", "unrelated-feature"])
    let notifications = TerminalNotifications(driver: driver)
    notifications.activate()
    await awaitNotification { !driver.withdrawn.isEmpty }
    #expect(driver.withdrawn == ["terminal-old-runtime"])
}
