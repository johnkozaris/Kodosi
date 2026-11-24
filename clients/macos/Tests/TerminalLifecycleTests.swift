import CoreGraphics
import Foundation
import KodosiBridge
@testable import KodosiDesktop
import KodosiTerminal
import KodosiTerminalSessions
import Observation
import Testing

@Test func `a terminal list that says the same changes nothing on screen`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "SpinnerFrame"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeReady(epoch: 0, userId: nil))
    func snapshot(activity: String) throws {
        try app.receive(runtimeEvent("sessions.snapshot", epoch: 0, userId: nil, fields: ["sessions": .array([.object([
            "id": .string("01992e43-53db-7040-8e02-f6ebf4149b28"), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
            "kind": .string("local"), "name": .string("Amber Wren"), "activity": .string(activity), "isOwner": .bool(true),
            "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
            "connectionState": .string("local"), "sharedWith": .array([]),
        ])])]))
    }
    try snapshot(activity: "Draft the release notes")
    var changed = false
    withObservationTracking {
        _ = app.sessions
    } onChange: {
        MainActor.assumeIsolated { changed = true }
    }
    try snapshot(activity: "Draft the release notes")
    #expect(!changed)
    try snapshot(activity: "Ship the release")
    #expect(changed)
    #expect(app.sessions.first?.activity == "Ship the release")
}

@Test func `releasing renderer cancels its connection without closing the session`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "RendererRelease"))
    let runtime = RuntimeHandle(inbox: RuntimeEventInbox())
    let commands = CommandSink { _ in Issue.record("Renderer release must not send session.close"); return 0 }
    var disconnected: [String] = []
    let manager = TerminalSessionManager(runtimeHandle: runtime, commandSink: commands,
                                         settings: DesktopSettings(defaults: defaults),
                                         disconnectTerminal: { id, _, _ in disconnected.append(id) })
    let id = UUIDv7.generate()
    let incarnation = UUIDv7.generate()
    manager.reconcileSessions(liveSessionIds: [id], runtimeIncarnations: [id: incarnation], resizeAuthority: [id: true])
    let first = manager.session(for: id)
    manager.releaseSession(for: id)
    #expect(!first.token.isActive)
    #expect(disconnected == [id])
    let reopened = manager.session(for: id)
    #expect(reopened !== first)
    #expect(reopened.runtimeIncarnationId == incarnation)
    #expect(reopened.token.subscriptionGeneration > first.token.subscriptionGeneration)
    manager.releaseSession(for: id)
}
