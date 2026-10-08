import Foundation
@testable import KodosiDesktop
import Testing

@Test func terminalSubscriptionTokensAreCanonicalUUIDs() throws {
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

@Test func inputSuppressionDoesNotRetireTheRendererConnection() {
    let token = TerminalConnectionToken()
    #expect(!token.canSendInput)
    token.setInputAllowed(true)
    #expect(token.canSendInput)
    token.setInputAllowed(false)
    #expect(token.isActive)
    #expect(!token.canSendInput)
    token.setInputAllowed(true)
    #expect(token.deactivate())
    #expect(!token.canSendInput)
    #expect(!token.deactivate())
}

@Test @MainActor func focusBlurAndResizeUseCanonicalSubscriptionToken() throws {
    let token = TerminalConnectionToken()
    let sessionId = UUIDv7.generate()
    let incarnationId = UUIDv7.generate()
    let requestId = UUIDv7.generate()
    var sent: [Data] = []
    let commands = CommandSink { data in sent.append(data); return 0 }
    #expect(commands.sendTerminalFocus(
        sessionId: sessionId, clientId: token.subscriptionId, subscriptionGeneration: token.subscriptionGeneration,
        requestId: requestId, expectedRuntimeIncarnationId: incarnationId
    ) == 0)
    #expect(commands.sendTerminalBlur(
        sessionId: sessionId, clientId: token.subscriptionId, subscriptionGeneration: token.subscriptionGeneration,
        expectedRuntimeIncarnationId: incarnationId
    ) == 0)
    commands.sendTerminalResize(sessionId: sessionId, identity: TerminalResizeIdentity(
        requestId: requestId, expectedRuntimeIncarnationId: incarnationId,
        subscriptionId: token.subscriptionId, subscriptionGeneration: token.subscriptionGeneration,
        surfaceGeneration: 1, cols: 80, rows: 24, widthPixels: 640, heightPixels: 384,
        cellWidthPixels: 8, cellHeightPixels: 16
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
