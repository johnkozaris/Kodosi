import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test func `product names use shared bounds`() {
    #expect(ProductInput.validName(" Terminal "))
    #expect(ProductInput.validName(String(repeating: "é", count: 64)))
    #expect(!ProductInput.validName(String(repeating: "é", count: 65)))
    #expect(!ProductInput.validName("\n\t "))
    #expect(!ProductInput.validName("one\ntwo"))
}

@Test @MainActor func `a hidden terminal leaves the list and the stage until it is back but stays in its room`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "HiddenTerminal"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    let id = "01992e43-53db-7040-8e02-f6ebf4149b28"
    func snapshot(hidden: Bool, back: Bool = false) throws -> RuntimeEvent {
        try runtimeEvent("sessions.snapshot", fields: ["sessions": .array([.object([
            "id": .string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
            "kind": .string("remote"), "name": .string("Copper Wren"), "isOwner": .bool(true), "hostName": .string("minisforum"),
            "status": .string(back ? "running" : "offline"), "attention": .bool(false), "hidden": .bool(hidden),
            "connectionState": .string(back ? "connected" : "offline"), "missionId": .string("room"), "sharedWith": .array([]),
        ])])])
    }
    try app.receive(snapshot(hidden: false))
    let away = try #require(app.session(id))
    #expect(away.hostOffline && away.canClose && !away.canControl)
    app.workbench.showSession(id)
    #expect(app.workbench.stagedSessionIds == [id])
    try app.receive(snapshot(hidden: true))
    #expect(app.workbench.stagedSessionIds.isEmpty)
    #expect(app.listedSessions.isEmpty)
    #expect(app.sessions.map(\.missionId) == ["room"])
    try app.receive(snapshot(hidden: false, back: true))
    #expect(app.listedSessions.map(\.id) == [id])
}

@Test @MainActor func `a saved sign-in that comes back is not shown as signed out`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "RestoringSignIn"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    try app.receive(runtimeEvent("auth.restoring", epoch: 0, userId: nil, fields: ["reason": .string("offline")]))
    #expect(app.account.restoring == "offline")
    #expect(app.signInPrompt == String(localized: "Signing in…"))
    try app.receive(runtimeEvent("auth.ready", epoch: 1, fields: ["userId": .string("owner")]))
    #expect(app.account.restoring == nil)
    #expect(app.account.userId == "owner")
}

@Test @MainActor func `signed out startup and refresh only list local sessions`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "SignedOutRefresh"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    var sent: [String] = []
    app.commandSink.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data), let type = command["type"]?.stringValue {
            sent.append(type)
        }
        return 0
    }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    #expect(sent == ["session.list"])
    try app.receive(runtimeEvent("auth.finalizing", epoch: 0, userId: nil))
    app.refresh()
    #expect(sent == ["session.list", "session.list"])
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("signedOut")]))
    app.refresh()
    #expect(sent.allSatisfy { $0 == "session.list" })
    #expect(app.errorMessage == nil)
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    sent.removeAll()
    app.refresh()
    #expect(Set(sent) == ["session.list", "auth.refresh", "friends.refresh", "devices.refresh", "mission.list"])
    app.shutdownProcess()
}

@Test @MainActor func `cold deep link waits for authentication then opens remote`() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "ColdLink"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    let id = "01992e43-53db-7040-8e02-f6ebf4149b28"
    let url = try #require(URL(string: "kodosi://session/\(id)"))
    app.pendingDeepLink = DeepLinkRouter.destination(for: url)
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    try app.receive(runtimeEvent("auth.finalizing", epoch: 0, userId: nil))
    try app.receive(runtimeEvent("sessions.snapshot", epoch: 0, userId: nil, fields: ["sessions": .array([])]))
    #expect(app.pendingDeepLink?.sessionId == id)
    #expect(!sent.contains { $0["type"] == .string("session.openRemote") })
    try app.receive(runtimeEvent("auth.ready", epoch: 1, fields: ["userId": .string("owner")]))
    for _ in 0 ..< 100 where !sent.contains(where: { $0["type"] == .string("session.openRemote") }) {
        await Task.yield()
    }
    let command = try #require(sent.first { $0["type"] == .string("session.openRemote") })
    #expect(command["sessionId"] == .string(id))
    #expect(command["accountEpoch"] == .int(1))
    #expect(app.pendingDeepLink == nil)
    let entry: JSONValue = .object([
        "id": .string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
        "kind": .string("remote"), "name": .string("Remote terminal"), "isOwner": .bool(true),
        "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "connectionState": .string("connected"), "sharedWith": .array([]),
    ])
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([entry])]))
    try app.receive(runtimeEvent("session.result", fields: [
        "requestId": #require(command["requestId"]), "operation": .string("session.openRemote"), "sessionId": .string(id),
    ]))
    for _ in 0 ..< 100 where app.workbench.selectedSessionId != id {
        await Task.yield()
    }
    #expect(app.workbench.selectedSessionId == id)
    #expect(app.workbench.stagedSessionIds == [id])
    app.shutdownProcess()
}

@Test @MainActor func `changed account drops old catalog and directory state`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "AccountFence"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    app.sessions = try [runtimeSession()]
    try app.receive(runtimeEvent("devices.list", fields: [
        "selfDeviceId": .string("this"), "localDeviceEnrolled": .bool(false), "devices": .array([]),
        "notice": .string("old account notice"),
    ]))
    #expect(app.account.identityMessage == "old account notice")
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.sessions.isEmpty)
    #expect(app.account.selfDeviceId == nil)
    #expect(app.account.identityMessage == nil)
    try app.receive(runtimeEvent("friends.snapshot", fields: [
        "friends": .array([.object([
            "userId": .string("other"), "handle": .string("other"), "displayName": .string("Other"),
            "verified": .bool(false), "identityState": .string("changed"),
        ])]),
        "incoming": .array([]), "outgoing": .array([]),
    ]))
    #expect(app.people.friends.isEmpty)
    app.shutdownProcess()
}

@Test @MainActor func `friend snapshot carries identity state and only a requested invite is copied`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "FriendIdentity"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    var copied: [String] = []
    app.copyText = { copied.append($0) }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    try app.receive(runtimeEvent("friends.snapshot", fields: [
        "friends": .array([
            .object([
                "userId": .string("a"), "handle": .string("ann"), "displayName": .string("Ann"),
                "verified": .bool(true), "identityState": .string("fixed"),
            ]),
            .object([
                "userId": .string("b"), "handle": .string("ben"), "displayName": .string("Ben"),
                "verified": .bool(false), "identityState": .string("changed"),
            ]),
        ]),
        "incoming": .array([]), "outgoing": .array([]),
    ]))
    #expect(app.people.friends.map(\.verified) == [true, false])
    #expect(app.people.friends.map(\.identityChanged) == [false, true])
    let invite: [String: JSONValue] = ["operation": .string("friends.invite"), "requestId": .string(UUIDv7.generate()), "text": .string("kodosi:owner:abcd")]
    try app.receive(runtimeEvent("friends.result", fields: invite))
    #expect(copied.isEmpty)
    app.copyInvite()
    try app.receive(runtimeEvent("friends.result", fields: invite))
    try app.receive(runtimeEvent("friends.result", fields: invite))
    #expect(copied == ["kodosi:owner:abcd"])
    #expect(app.inviteCopies == 1)
    #expect(app.people.friends.count == 2)
}

@Test @MainActor func `kodosi keeps running after its window closes until the person turns it off`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "KeepRunning"))
    let settings = DesktopSettings(defaults: defaults)
    #expect(settings.keepRunning)
    settings.keepRunning = false
    #expect(!DesktopSettings(defaults: defaults).keepRunning)
}
