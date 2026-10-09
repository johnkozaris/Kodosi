import Foundation
@testable import KodosiDesktop
import Testing

@Test func productNamesUseSharedBounds() {
    #expect(ProductInput.validName(" Terminal "))
    #expect(ProductInput.validName(String(repeating: "é", count: 64)))
    #expect(!ProductInput.validName(String(repeating: "é", count: 65)))
    #expect(!ProductInput.validName("\n\t "))
    #expect(!ProductInput.validName("one\ntwo"))
}

@Test @MainActor func signedOutStartupAndRefreshOnlyListLocalSessions() throws {
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
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(52)]))
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

@Test @MainActor func coldDeepLinkWaitsForAuthenticationThenOpensRemote() async throws {
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
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(52)]))
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
        "status": .string("running"), "connectionState": .string("connected"), "sharedWith": .array([]),
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

@Test @MainActor func changedAccountDropsOldCatalogAndDirectoryState() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "AccountFence"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    app.sessions = try [runtimeSession()]
    app.localDeviceEnrolled = true
    app.identityMessage = "old account notice"
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.sessions.isEmpty)
    #expect(!app.localDeviceEnrolled)
    #expect(app.identityMessage == nil)
    try app.receive(runtimeEvent("friends.snapshot", fields: [
        "friends": .array([.object([
            "userId": .string("other"), "handle": .string("other"), "displayName": .string("Other"),
            "verified": .bool(false), "identityState": .string("changed"),
        ])]),
        "incoming": .array([]), "outgoing": .array([]),
    ]))
    #expect(app.friends.isEmpty)
    app.shutdownProcess()
}

@Test @MainActor func friendSnapshotCarriesIdentityStateAndTheInviteTextIsKept() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "FriendIdentity"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
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
    #expect(app.friends.map(\.verified) == [true, false])
    #expect(app.friends.map(\.identityChanged) == [false, true])
    try app.receive(runtimeEvent("friends.invite", fields: ["text": .string("kodosi:owner:abcd")]))
    #expect(app.ownInvite == "kodosi:owner:abcd")
    #expect(app.friends.count == 2)
}
