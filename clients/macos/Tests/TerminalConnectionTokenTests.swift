import Foundation
import KodosiBridge
import KodosiTerminalSessions
import Testing

@Test func `terminal subscription tokens are canonical UUIDs`() throws {
    let first = TerminalConnectionToken()
    let replacement = TerminalConnectionToken()
    for token in [first, replacement] {
        let parsed = try #require(UUID(uuidString: token.subscriptionId))
        #expect(token.subscriptionId == parsed.uuidString.lowercased())
        #expect(parsed.uuid.6 >> 4 == 7)
        #expect(token.subscriptionGeneration > 0)
    }
    #expect(first.subscriptionId != replacement.subscriptionId)
    #expect(first.subscriptionGeneration < replacement.subscriptionGeneration)
}

@Test func `input suppression does not retire the renderer connection`() {
    let token = TerminalConnectionToken()
    let view = UUID()
    #expect(!token.canSendInput)
    token.setInputAllowed(true, by: view)
    #expect(token.canSendInput)
    token.setInputAllowed(false, by: view)
    #expect(token.isActive)
    #expect(!token.canSendInput)
    token.setInputAllowed(true, by: view)
    #expect(token.deactivate())
    #expect(!token.canSendInput)
    #expect(!token.deactivate())
}

@Test func `a view that goes away does not stop the input of the view that replaced it`() {
    let token = TerminalConnectionToken()
    let grid = UUID()
    let room = UUID()
    token.setInputAllowed(true, by: grid)
    token.setInputAllowed(true, by: room)
    token.setInputAllowed(false, by: grid)
    #expect(token.canSendInput)
    token.setInputAllowed(false, by: room)
    #expect(!token.canSendInput)
}

@Test @MainActor func `focus blur and resize use canonical subscription token`() throws {
    let token = TerminalConnectionToken()
    let sessionId = UUIDv7.generate()
    let incarnationId = UUIDv7.generate()
    let requestId = UUIDv7.generate()
    var sent: [Data] = []
    let commands = CommandSink { data in sent.append(data); return 0 }
    #expect(commands.sendTerminalFocus(
        sessionId: sessionId, clientId: token.subscriptionId, subscriptionGeneration: token.subscriptionGeneration,
        requestId: requestId, expectedRuntimeIncarnationId: incarnationId,
    ) == 0)
    #expect(commands.sendTerminalBlur(
        sessionId: sessionId, clientId: token.subscriptionId, subscriptionGeneration: token.subscriptionGeneration,
        expectedRuntimeIncarnationId: incarnationId,
    ) == 0)
    commands.sendTerminalResize(sessionId: sessionId, identity: TerminalResizeIdentity(
        requestId: requestId, expectedRuntimeIncarnationId: incarnationId,
        subscriptionId: token.subscriptionId, subscriptionGeneration: token.subscriptionGeneration,
        surfaceGeneration: 1, cols: 80, rows: 24, widthPixels: 640, heightPixels: 384,
        cellWidthPixels: 8, cellHeightPixels: 16,
    ))
    #expect(sent.count == 3)
    for data in sent {
        let fields = try JSONDecoder().decode([String: JSONValue].self, from: data)
        let idKey = fields["type"] == .string("session.resize") ? "subscriptionId" : "clientId"
        let subscriptionId = try #require(fields[idKey]?.stringValue)
        #expect(subscriptionId == token.subscriptionId)
        #expect(subscriptionId == UUID(uuidString: subscriptionId)?.uuidString.lowercased())
        #expect(fields["subscriptionGeneration"] == .int(Int64(token.subscriptionGeneration)))
        #expect(fields["sessionId"] == .string(sessionId))
        #expect(fields["expectedRuntimeIncarnationId"] == .string(incarnationId))
    }
}
