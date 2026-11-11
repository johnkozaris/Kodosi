import Foundation
import KodosiBridge
import KodosiDesign
import Testing

private let people = [
    ProjectMention(id: "u1", name: "Leo Park", kind: .person(isSelf: true)),
    ProjectMention(id: "u2", name: "Maya Chen", kind: .person(isSelf: false)),
    ProjectMention(id: "u3", name: "Maya", kind: .person(isSelf: false)),
    ProjectMention(id: "t1", name: "Amber Wren", kind: .terminal(.claude)),
]

@Test func `mentions match the longest known name at a word boundary`() {
    let segments = ProjectMentions.segments("Hi @maya chen and @Amber Wren, mail a@Maya.com @Mayan", targets: people)
    #expect(segments == [
        .text("Hi "), .mention(people[1]), .text(" and "), .mention(people[3]), .text(", mail a@Maya.com @Mayan"),
    ])
    #expect(ProjectMentions.segments("No mention here", targets: people) == [.text("No mention here")])
    #expect(ProjectMentions.segments("@Maya!", targets: people) == [.mention(people[2]), .text("!")])
    #expect(ProjectMentions.mentionsSelf("ping @Leo Park", targets: people))
    #expect(!ProjectMentions.mentionsSelf("ping @Maya", targets: people))
}

@Test func `a person is mentioned by the user name too`() {
    let leo = ProjectMention(id: "u1", name: "Leo Park", kind: .person(isSelf: true), handle: "leo")
    #expect(ProjectMentions.segments("thanks @leo.", targets: [leo]) == [.text("thanks "), .mention(leo), .text(".")])
    #expect(ProjectMentions.mentionsSelf("@Leo Park and @leo", targets: [leo]))
    #expect(!ProjectMentions.mentionsSelf("@leopard", targets: [leo]))
}

@Test func `mention drafts complete the name after the last at sign`() {
    #expect(ProjectMentions.query(in: "Thanks @ma") == "ma")
    #expect(ProjectMentions.query(in: "@") == "")
    #expect(ProjectMentions.query(in: "mail a@b") == nil)
    #expect(ProjectMentions.query(in: "no sign") == nil)
    #expect(ProjectMentions.query(in: "@ma\nnext") == nil)
    #expect(ProjectMentions.query(in: "@Maya Chen thanks for the long answer") == nil)
    #expect(ProjectMentions.suggestions(for: "ma", in: people).map(\.id) == ["u2", "u3"])
    #expect(ProjectMentions.suggestions(for: "", in: people).map(\.id) == ["u2", "u3", "t1"])
    #expect(ProjectMentions.suggestions(for: "wren", in: people).map(\.id) == ["t1"])
    #expect(ProjectMentions.suggestions(for: "leo", in: people).isEmpty)
    #expect(ProjectMentions.complete("Thanks @ma", with: people[1]) == "Thanks @Maya Chen ")
    #expect(ProjectMentions.start("") == "@")
    #expect(ProjectMentions.start("hi") == "hi @")
    #expect(ProjectMentions.start("hi ") == "hi @")
    #expect(ProjectMentions.start("hi @") == "hi @")
}

@Test func `terminal mention links round trip`() throws {
    let link = try #require(people[3].link)
    #expect(link.absoluteString == "kodosi-project://terminal/t1")
    #expect(ProjectMentions.terminalId(in: link) == "t1")
    #expect(try ProjectMentions.terminalId(in: #require(URL(string: "https://example.com/terminal/t1"))) == nil)
    #expect(people[1].link == nil)
}

@MainActor
@Test func `message text links addresses`() {
    let text = ProjectMessageText.formatted("See https://github.com/team/app/pull/7 and **this**")
    #expect(text.runs.compactMap(\.link).map(\.absoluteString) == ["https://github.com/team/app/pull/7"])
    #expect(String(text.characters) == "See https://github.com/team/app/pull/7 and this")
}

@Test func `messages group by author and time`() throws {
    func message(_ id: String, author: String, at date: String) throws -> ProjectMessage {
        try JSONValue.object([
            "id": .string(id), "sequence": .uint(1), "authorId": .string(author), "authorName": .string("Ana"),
            "agent": .null, "terminalId": .null, "text": .string("Hello"), "createdAt": .string(date),
        ]).decode(as: ProjectMessage.self)
    }
    let first = try message("m1", author: "u2", at: "2026-10-11T10:00:00Z")
    let soon = try message("m2", author: "u2", at: "2026-10-11T10:04:00Z")
    let late = try message("m3", author: "u2", at: "2026-10-11T10:20:00Z")
    let other = try message("m4", author: "u3", at: "2026-10-11T10:21:00Z")
    #expect(ProjectDates.startsGroup(first, after: nil))
    #expect(!ProjectDates.startsGroup(soon, after: first))
    #expect(ProjectDates.startsGroup(late, after: soon))
    #expect(ProjectDates.startsGroup(other, after: late))
    #expect(ProjectDates.startsDay(first, after: nil))
    #expect(!ProjectDates.startsDay(soon, after: first))
}
