import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiDesign
import Testing

@MainActor
struct RoomMentionsTests {
    private let targets = [
        RoomMention(id: "user-1", name: "Ana Lee", detail: "@ana", kind: .person(isSelf: true)),
        RoomMention(id: "user-2", name: "Bo", detail: "@bo", kind: .person(isSelf: false)),
        RoomMention(id: "term-1", name: "api server", detail: "Claude Code", kind: .terminal(.claude)),
    ]

    @Test func segmentsFindPeopleAndTerminals() {
        let segments = RoomMentions.segments("Hi @ana lee, see @api server now", targets: targets)
        #expect(segments == [.text("Hi "), .mention(targets[0]), .text(", see "), .mention(targets[2]), .text(" now")])
    }

    @Test func mentionsStartAndEndAtWordEdges() {
        #expect(RoomMentions.segments("mail@Bo", targets: targets) == [.text("mail@Bo")])
        #expect(RoomMentions.segments("@Bob", targets: targets) == [.text("@Bob")])
        #expect(RoomMentions.segments("@Bo!", targets: targets) == [.mention(targets[1]), .text("!")])
    }

    @Test func selfMentionTintsTheMessage() {
        #expect(RoomMentions.mentionsSelf("thanks @Ana Lee", targets: targets))
        #expect(!RoomMentions.mentionsSelf("thanks @Bo", targets: targets))
    }

    @Test func queryFollowsTheLastAt() {
        #expect(RoomMentions.query(in: "hey @an") == "an")
        #expect(RoomMentions.query(in: "hey @") == "")
        #expect(RoomMentions.query(in: "mail@an") == nil)
        #expect(RoomMentions.query(in: "@a b c d") == nil)
        #expect(RoomMentions.query(in: "@an\nnext") == nil)
        #expect(RoomMentions.query(in: "no mention") == nil)
    }

    @Test func completingReplacesTheQuery() {
        #expect(RoomMentions.complete("hey @ap", with: targets[2]) == "hey @api server ")
        #expect(RoomMentions.start("") == "@")
        #expect(RoomMentions.start("hi") == "hi @")
        #expect(RoomMentions.start("hi ") == "hi @")
        #expect(RoomMentions.start("hi @") == "hi @")
    }

    @Test func suggestionsSkipYouAndPreferPrefixes() {
        #expect(RoomMentions.suggestions(for: "", in: targets).map(\.id) == ["user-2", "term-1"])
        #expect(RoomMentions.suggestions(for: "a", in: targets).map(\.id) == ["term-1"])
        #expect(RoomMentions.suggestions(for: "serv", in: targets).map(\.id) == ["term-1"])
        #expect(RoomMentions.suggestions(for: "ana", in: targets).isEmpty)
    }

    @Test func terminalLinksRoundTrip() throws {
        let link = try #require(RoomMentions.terminalLink("term-1"))
        #expect(link.absoluteString == "kodosi-room://terminal/term-1")
        #expect(RoomMentions.terminalId(in: link) == "term-1")
        let web = try #require(URL(string: "https://example.com/terminal/term-1"))
        #expect(RoomMentions.terminalId(in: web) == nil)
        #expect(targets[2].link == link)
        #expect(targets[1].link == nil)
    }

    @Test func messageTextLinksAddresses() {
        let text = RoomMessageText.formatted("See https://github.com/team/app/pull/7 and **this**")
        let links = text.runs.compactMap(\.link)
        #expect(links.map(\.absoluteString) == ["https://github.com/team/app/pull/7"])
        #expect(String(text.characters) == "See https://github.com/team/app/pull/7 and this")
    }

    @Test func messagesGroupByAuthorAndTime() throws {
        let first = try message("m1", author: "user-2", at: "2026-10-11T10:00:00Z")
        let soon = try message("m2", author: "user-2", at: "2026-10-11T10:04:00Z")
        let late = try message("m3", author: "user-2", at: "2026-10-11T10:20:00Z")
        let other = try message("m4", author: "user-3", at: "2026-10-11T10:21:00Z")
        #expect(RoomDates.startsGroup(first, after: nil))
        #expect(!RoomDates.startsGroup(soon, after: first))
        #expect(RoomDates.startsGroup(late, after: soon))
        #expect(RoomDates.startsGroup(other, after: late))
        #expect(RoomDates.startsDay(first, after: nil))
        #expect(!RoomDates.startsDay(soon, after: first))
    }

    private func message(_ id: String, author: String, at date: String) throws -> RoomMessage {
        try JSONValue.object([
            "id": .string(id), "sequence": .uint(1), "authorId": .string(author), "authorName": .string("Ana"),
            "agent": .null, "terminalId": .null, "text": .string("Hello"), "createdAt": .string(date),
        ]).decode(as: RoomMessage.self)
    }
}
