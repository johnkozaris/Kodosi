import Foundation
import KodosiTerminalSessions
import Testing

@Test func `terminal subscription tokens are canonical UUIDs`() throws {
    let first = TerminalConnectionToken()
    let replacement = TerminalConnectionToken()
    for token in [first, replacement] {
        let parsed = try #require(UUID(uuidString: token.subscriptionId))
        #expect(token.subscriptionId == parsed.uuidString.lowercased())
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
    let project = UUID()
    token.setInputAllowed(true, by: grid)
    token.setInputAllowed(true, by: project)
    token.setInputAllowed(false, by: grid)
    #expect(token.canSendInput)
    token.setInputAllowed(false, by: project)
    #expect(!token.canSendInput)
}
