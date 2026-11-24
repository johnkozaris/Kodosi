import Foundation
@testable import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing

private func item(_ name: String, changed: UInt64) -> JSONValue {
    .object([
        "key": .string("/tmp/work/\(name)"), "name": .string(name), "kind": .string("page"),
        "revision": .uint(1), "bytes": .uint(120), "changed": .uint(changed),
    ])
}

private func terminal(canvas: [String: JSONValue]?) throws -> RuntimeSession {
    var fields: [String: JSONValue] = [
        "id": .string("01992e43-53db-7040-8e02-f6ebf4149b28"), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("checkout"), "program": .string("claude"),
        "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
        "connectionState": .string("local"), "sharedWith": .array([]),
    ]
    fields["canvas"] = canvas.map(JSONValue.object)
    return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
}

private let notes = CanvasItem(key: "notes.txt", name: "notes.txt", kind: .document, view: .text, revision: 1, bytes: 4, changed: 1)

@Test func `a document whose frames break takes its pipe again, and the parts on their way go nowhere`() async throws {
    let commands = CommandSink()
    var sent: [[String: JSONValue]] = []
    commands.transport { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    let store = CanvasStore(commands: commands)
    let link = store.document(notes, in: "01992e43-53db-7040-8e02-f6ebf4149b28")
    // This test gives the pipe its events on the main actor.
    nonisolated(unsafe) var news: [DocumentLink.News] = []
    nonisolated(unsafe) var frames = 0
    link.onNews = { news.append($0) }
    link.onFrame = { _ in frames += 1 }
    let opening = Task { try await link.open() }
    while link.pipe == nil {
        await Task.yield()
    }
    let pipe = try #require(link.pipe)
    let frame = { (sequence: UInt64) in
        try store.receive(runtimeEvent("document.frame", fields: [
            "pipe": .string(pipe), "sequence": .uint(sequence), "more": .bool(false), "data": .string("AAAA"),
        ]))
    }
    try store.receive(runtimeEvent("document.opened", fields: ["pipe": .string(pipe)]))
    try frame(0)
    try frame(3)
    try frame(4)
    while sent.last?["type"] != .string("document.open") {
        await Task.yield()
    }
    #expect(sent.suffix(2).map { $0["type"] } == [.string("document.close"), .string("document.open")])
    #expect(sent.suffix(2).allSatisfy { $0["pipe"] == .string(pipe) })
    try store.receive(runtimeEvent("document.opened", fields: ["pipe": .string(pipe)]))
    try frame(0)
    #expect(news == [.open, .open] && frames == 2)
    opening.cancel()
    _ = await opening.result
}

@Test func `after the app falls behind, an open pipe and a pipe whose answer went missing both start again`() async throws {
    let commands = CommandSink()
    var sent: [[String: JSONValue]] = []
    commands.transport { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    let store = CanvasStore(commands: commands)
    let open = store.document(notes, in: "01992e43-53db-7040-8e02-f6ebf4149b28")
    let opening = Task { try await open.open() }
    while sent.isEmpty {
        await Task.yield()
    }
    let first = try #require(open.pipe)
    try commands.receive(runtimeEvent("document.result", fields: [
        "requestId": #require(sent.last?["requestId"]), "operation": .string("document.open"),
    ]))
    try await opening.value
    try store.receive(runtimeEvent("document.opened", fields: ["pipe": .string(first)]))
    let waiting = store.document(notes, in: "01992e43-53db-7040-8e02-f6ebf4149b29")
    let registering = Task { try await waiting.open() }
    while sent.count < 2 {
        await Task.yield()
    }
    let second = try #require(waiting.pipe)
    let unanswered = try #require(sent.last?["requestId"])
    sent.removeAll()
    commands.abandonPending()
    store.renewPipes()
    while sent.count(where: { $0["type"] == .string("document.open") }) < 2 {
        await Task.yield()
    }
    for pipe in [first, second] {
        #expect(sent.filter { $0["pipe"] == .string(pipe) }.map { $0["type"] } == [.string("document.close"), .string("document.open")])
    }
    let late = try runtimeEvent("document.result", fields: ["requestId": unanswered, "operation": .string("document.open")])
    #expect(commands.receive(late) == .caller)
    registering.cancel()
    _ = await registering.result
}

@Test func `ask about this types one line for the person to finish`() throws {
    let session = try terminal(canvas: ["sequence": .uint(1), "items": .array([item("plan.md", changed: 1)])])
    let shown = try #require(session.canvas?.shown.first)
    #expect(CanvasQuestion.about(shown, words: nil) == "About /tmp/work/plan.md: ")
    #expect(CanvasQuestion.about(shown, words: " round\neach\u{1B}[2J line\r") == "About /tmp/work/plan.md: \"round each [2J line\" ")
    #expect(CanvasQuestion.about(shown, words: "  ") == "About /tmp/work/plan.md: ")
}
