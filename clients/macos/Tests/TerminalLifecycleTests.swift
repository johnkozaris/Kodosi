import CoreGraphics
import Foundation
@testable import KodosiDesktop
import Observation
import Testing

@Test @MainActor func createdSessionHasIncarnationBeforePresentationConstructsRenderer() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "CreatedRenderer"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(54)]))
    let id = UUIDv7.generate()
    let incarnation = UUIDv7.generate()
    var renderer: ManagedTerminalSession?
    withObservationTracking {
        _ = app.workbench.selectedSessionId
    } onChange: {
        MainActor.assumeIsolated { renderer = app.terminalManager.session(for: id) }
    }
    var requestId: String?
    app.commandSink.transport = { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data), fields["type"] == .string("session.create") {
            requestId = fields["requestId"]?.stringValue
        }
        return 0
    }
    let creating = Task { try await app.createSession(name: "Test", directory: nil) }
    for _ in 0 ..< 100 where requestId == nil {
        await Task.yield()
    }
    let request = try #require(requestId)
    let entry: JSONValue = .object([
        "id": .string(id), "incarnationId": .string(incarnation), "kind": .string("local"), "name": .string("Test"),
        "isOwner": .bool(true), "status": .string("running"), "connectionState": .string("local"),
        "sharedWith": .array([]), "createRequestId": .string(request),
    ])
    try app.receive(runtimeEvent("sessions.snapshot", epoch: 0, userId: nil, fields: ["sessions": .array([entry])]))
    let managed = try #require(renderer)
    #expect(managed.runtimeIncarnationId == incarnation)
    #expect(managed.token.isActive)
    #expect(managed.resizeAuthority.isAllowed)
    #expect(app.terminalManager.session(for: id) === managed)
    try app.receive(runtimeEvent("session.result", epoch: 0, userId: nil, fields: [
        "requestId": .string(request), "sessionId": .string(id), "operation": .string("session.create"),
    ]))
    try await creating.value
}

@Test @MainActor func releasingRendererCancelsItsConnectionWithoutClosingTheSession() throws {
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

@Test @MainActor func terminalTilesFillTheStageWithoutChangingSessionOrder() {
    for size in [CGSize(width: 960, height: 660), CGSize(width: 1400, height: 800), CGSize(width: 700, height: 500)] {
        for count in 1 ... 6 {
            let bounds = CGRect(origin: .zero, size: size)
            let frames = TerminalTilesLayout.frames(count: count, in: bounds)
            #expect(frames.count == count)
            #expect(frames.first?.origin == .zero)
            #expect(abs((frames.last?.maxY ?? 0) - bounds.maxY) < 0.01)
            for (index, frame) in frames.enumerated() {
                #expect(bounds.contains(frame))
                #expect(frame.width > 0 && frame.height > 0)
                for other in frames.dropFirst(index + 1) {
                    #expect(!frame.intersects(other))
                }
            }
        }
    }
}
