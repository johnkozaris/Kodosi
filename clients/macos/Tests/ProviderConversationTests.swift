import Foundation
@testable import KodosiDesktop
import Testing

@Test func resumeSessionNameFallsBackWithoutChangingHistoryTitle() {
    let invalidTitles: [String?] = [
        nil, "", " \n\t ", "task\nwith control", "task\u{0000}title",
        String(repeating: "a", count: 129), String(repeating: "é", count: 65),
    ]
    for provider in Provider.allCases {
        for title in invalidTitles {
            let conversation = ProviderConversation(
                provider: provider, nativeConversationId: "native-conversation", workingDirectory: "/project",
                title: title, createdAt: nil, updatedAt: nil
            )
            #expect(conversation.sessionName == provider.name)
            #expect(ProductInput.validName(conversation.sessionName))
            #expect(conversation.title == title)
            #expect(conversation.identity.nativeConversationId == "native-conversation")
        }
    }
}

@Test func resumeSessionNamePreservesValidUTF8BoundaryAndTrimsOnlyName() {
    for title in ["Saved work", String(repeating: "a", count: 128), String(repeating: "é", count: 64), "  Saved work  "] {
        let conversation = ProviderConversation(
            provider: .claude, nativeConversationId: "native-conversation", workingDirectory: "/project",
            title: title, createdAt: nil, updatedAt: nil
        )
        #expect(conversation.sessionName == title.trimmingCharacters(in: .whitespacesAndNewlines))
        #expect(ProductInput.validName(conversation.sessionName))
        #expect(conversation.title == title)
    }
}
