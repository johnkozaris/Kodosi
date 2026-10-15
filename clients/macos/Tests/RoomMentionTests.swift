import Foundation
import KodosiBridge
import KodosiDesign
import Testing

private let people = [
    RoomMention(id: "u1", name: "Leo Park", kind: .person(isSelf: true)),
    RoomMention(id: "u2", name: "Maya Chen", kind: .person(isSelf: false)),
    RoomMention(id: "u3", name: "Maya", kind: .person(isSelf: false)),
    RoomMention(id: "t1", name: "Amber Wren", kind: .terminal(.claude)),
]

@Test func mentionsMatchTheLongestKnownNameAtAWordBoundary() {
    let segments = RoomMentions.segments("Hi @maya chen and @Amber Wren, mail a@Maya.com @Mayan", targets: people)
    #expect(segments == [
        .text("Hi "), .mention(people[1]), .text(" and "), .mention(people[3]), .text(", mail a@Maya.com @Mayan"),
    ])
    #expect(RoomMentions.segments("No mention here", targets: people) == [.text("No mention here")])
    #expect(RoomMentions.segments("@Maya!", targets: people) == [.mention(people[2]), .text("!")])
    #expect(RoomMentions.mentionsSelf("ping @Leo Park", targets: people))
    #expect(!RoomMentions.mentionsSelf("ping @Maya", targets: people))
}

@Test func mentionDraftsCompleteTheNameAfterTheLastAtSign() {
    #expect(RoomMentions.query(in: "Thanks @ma") == "ma")
    #expect(RoomMentions.query(in: "@") == "")
    #expect(RoomMentions.query(in: "mail a@b") == nil)
    #expect(RoomMentions.query(in: "no sign") == nil)
    #expect(RoomMentions.query(in: "@ma\nnext") == nil)
    #expect(RoomMentions.query(in: "@Maya Chen thanks for the long answer") == nil)
    #expect(RoomMentions.suggestions(for: "ma", in: people).map(\.id) == ["u2", "u3"])
    #expect(RoomMentions.suggestions(for: "", in: people).map(\.id) == ["u2", "u3", "t1"])
    #expect(RoomMentions.suggestions(for: "wren", in: people).map(\.id) == ["t1"])
    #expect(RoomMentions.suggestions(for: "leo", in: people).isEmpty)
    #expect(RoomMentions.complete("Thanks @ma", with: people[1]) == "Thanks @Maya Chen ")
    #expect(RoomMentions.start("") == "@")
    #expect(RoomMentions.start("hi") == "hi @")
    #expect(RoomMentions.start("hi ") == "hi @")
    #expect(RoomMentions.start("hi @") == "hi @")
}

@Test func terminalMentionLinksRoundTrip() throws {
    let link = try #require(people[3].link)
    #expect(link.absoluteString == "kodosi-room://terminal/t1")
    #expect(RoomMentions.terminalId(in: link) == "t1")
    #expect(try RoomMentions.terminalId(in: #require(URL(string: "https://example.com/terminal/t1"))) == nil)
    #expect(people[1].link == nil)
}

@MainActor
@Test func messageTextLinksAddresses() {
    let text = RoomMessageText.formatted("See https://github.com/team/app/pull/7 and **this**")
    #expect(text.runs.compactMap(\.link).map(\.absoluteString) == ["https://github.com/team/app/pull/7"])
    #expect(String(text.characters) == "See https://github.com/team/app/pull/7 and this")
}

@Test func messagesGroupByAuthorAndTime() throws {
    func message(_ id: String, author: String, at date: String) throws -> RoomMessage {
        try JSONValue.object([
            "id": .string(id), "sequence": .uint(1), "authorId": .string(author), "authorName": .string("Ana"),
            "agent": .null, "terminalId": .null, "text": .string("Hello"), "createdAt": .string(date),
        ]).decode(as: RoomMessage.self)
    }
    let first = try message("m1", author: "u2", at: "2026-10-11T10:00:00Z")
    let soon = try message("m2", author: "u2", at: "2026-10-11T10:04:00Z")
    let late = try message("m3", author: "u2", at: "2026-10-11T10:20:00Z")
    let other = try message("m4", author: "u3", at: "2026-10-11T10:21:00Z")
    #expect(RoomDates.startsGroup(first, after: nil))
    #expect(!RoomDates.startsGroup(soon, after: first))
    #expect(RoomDates.startsGroup(late, after: soon))
    #expect(RoomDates.startsGroup(other, after: late))
    #expect(RoomDates.startsDay(first, after: nil))
    #expect(!RoomDates.startsDay(soon, after: first))
}
