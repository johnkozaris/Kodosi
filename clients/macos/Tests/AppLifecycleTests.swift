import AppKit
import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test func `signed out startup and refresh only list local sessions`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "SignedOutRefresh"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    var sent: [String] = []
    app.commandSink.transport { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data), let type = command["type"]?.stringValue {
            sent.append(type)
        }
        return 0
    }
    try app.receive(runtimeReady(epoch: 0, userId: nil))
    #expect(sent == ["app.presence", "app.look", "session.list", "session.visible"])
    try app.receive(runtimeEvent("auth.finalizing", epoch: 0, userId: nil))
    app.refresh()
    #expect(sent == ["app.presence", "app.look", "session.list", "session.visible", "session.list"])
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("signedOut")]))
    app.refresh()
    #expect(sent.filter { $0 != "session.list" } == ["app.presence", "app.look", "session.visible", "session.restore"])
    #expect(app.errorMessage == nil)
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    sent.removeAll()
    app.refresh()
    #expect(Set(sent) == ["session.list", "auth.refresh", "friends.refresh", "devices.refresh", "project.list"])
    app.shutdownProcess()
}

@Test func `a remote terminal that is not on the stage when it opens is disconnected`() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "OpenedOffStage"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    let id = "01992e43-53db-7040-8e02-f6ebf4149b28"
    app.activateSession(id)
    for _ in 0 ..< 100 where !sent.contains(where: { $0["type"] == .string("session.openRemote") }) {
        await Task.yield()
    }
    let open = try #require(sent.first { $0["type"] == .string("session.openRemote") })
    app.workbench.section = .settings
    try app.receive(runtimeEvent("sessions.snapshot", fields: ["sessions": .array([.object([
        "id": .string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
        "kind": .string("remote"), "name": .string("Remote terminal"), "isOwner": .bool(true),
        "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
        "connectionState": .string("connected"), "sharedWith": .array([]),
    ])])]))
    try app.receive(runtimeEvent("session.result", fields: [
        "requestId": #require(open["requestId"]), "operation": .string("session.openRemote"), "sessionId": .string(id),
    ]))
    for _ in 0 ..< 100 where !sent.contains(where: { $0["type"] == .string("session.disconnect") }) {
        await Task.yield()
    }
    #expect(sent.last?["type"] == .string("session.disconnect"))
    #expect(sent.last?["sessionId"] == .string(id))
    #expect(app.workbench.stagedSessionIds.isEmpty)
}

@Test func `the host hears which terminals are in the window and whether the person is in the app`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "VisibleReport"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var reports: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data), command["type"] == .string("session.visible") {
            reports.append(command)
        }
        return 0
    }
    var onScreen = true
    app.windowShown = { onScreen }
    app.isFront = { false }
    let ids = ["01992e43-53db-7040-8e02-f6ebf4149b21", "01992e43-53db-7040-8e02-f6ebf4149b22"]
    app.sessions = try ids.map { try runtimeSession($0) }
    ids.forEach { app.workbench.showSession($0) }
    app.reportVisible()
    #expect(reports.last?["sessionIds"] == .array(ids.map(JSONValue.string)))
    #expect(reports.last?["watching"] == .bool(false))
    app.workbench.toggleFocus(ids[1])
    app.isFront = { true }
    app.reportVisible()
    #expect(reports.last?["sessionIds"] == .array([.string(ids[1])]))
    #expect(reports.last?["watching"] == .bool(true))
    onScreen = false
    app.reportVisible()
    #expect(reports.last?["sessionIds"] == .array([]))
    let sent = reports.count
    app.reportVisible()
    #expect(reports.count == sent)
}

@Test func `changed account drops old catalog and directory state`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "AccountFence"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    app.sessions = try [runtimeSession()]
    try app.receive(runtimeEvent("devices.list", fields: [
        "selfDeviceId": .string("this"), "localDeviceEnrolled": .bool(false), "devices": .array([]),
    ]))
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.sessions.isEmpty)
    #expect(app.account.selfDeviceId == nil)
    try app.receive(runtimeEvent("friends.snapshot", fields: [
        "friends": .array([.object([
            "userId": .string("other"), "handle": .string("other"), "displayName": .string("Other"),
            "verified": .bool(false), "identityState": .string("changed"), "inviteNeeded": .bool(false),
        ])]),
        "incoming": .array([]), "outgoing": .array([]), "blocked": .array([]),
    ]))
    #expect(app.people.friends.isEmpty)
    app.shutdownProcess()
}

@Test func `friend snapshot carries identity state and only a requested invite is copied`() throws {
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
                "verified": .bool(true), "identityState": .string("fixed"), "inviteNeeded": .bool(false),
            ]),
            .object([
                "userId": .string("b"), "handle": .string("ben"), "displayName": .string("Ben"),
                "verified": .bool(false), "identityState": .string("changed"), "inviteNeeded": .bool(false),
            ]),
        ]),
        "incoming": .array([]), "outgoing": .array([]), "blocked": .array([]),
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
