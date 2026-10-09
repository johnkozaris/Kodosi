import Foundation
@testable import KodosiDesktop
import Testing

private let people = [
    MentionTarget(id: "u1", name: "John Kozaris", kind: .person(isSelf: true)),
    MentionTarget(id: "u2", name: "Maya Chen", kind: .person(isSelf: false)),
    MentionTarget(id: "u3", name: "Maya", kind: .person(isSelf: false)),
    MentionTarget(id: "t1", name: "Amber Wren", kind: .terminal(.claude)),
]

@Test func mentionsMatchTheLongestKnownNameAtAWordBoundary() {
    let segments = RoomMentions.segments("Hi @maya chen and @Amber Wren, mail a@Maya.com @Mayan", targets: people)
    #expect(segments == [
        .text("Hi "), .mention(people[1]), .text(" and "), .mention(people[3]), .text(", mail a@Maya.com @Mayan"),
    ])
    #expect(RoomMentions.segments("No mention here", targets: people) == [.text("No mention here")])
    #expect(RoomMentions.mentionsSelf("ping @John Kozaris", targets: people))
    #expect(!RoomMentions.mentionsSelf("ping @Maya", targets: people))
    #expect(people[3].link?.absoluteString == "kodosi-mention://terminal/t1")
    #expect(people[1].link == nil)
}

@Test func mentionDraftsCompleteTheNameAfterTheLastAtSign() {
    #expect(RoomMentions.query(in: "Thanks @ma") == "ma")
    #expect(RoomMentions.query(in: "@") == "")
    #expect(RoomMentions.query(in: "mail a@b") == nil)
    #expect(RoomMentions.query(in: "no sign") == nil)
    #expect(RoomMentions.query(in: "@Maya Chen thanks for the long answer") == nil)
    #expect(RoomMentions.suggestions(for: "ma", in: people).map(\.id) == ["u2", "u3"])
    #expect(RoomMentions.suggestions(for: "", in: people).map(\.id) == ["u2", "u3", "t1"])
    #expect(RoomMentions.suggestions(for: "wren", in: people).map(\.id) == ["t1"])
    #expect(RoomMentions.complete("Thanks @ma", with: people[1]) == "Thanks @Maya Chen ")
}
