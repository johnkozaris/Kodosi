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
