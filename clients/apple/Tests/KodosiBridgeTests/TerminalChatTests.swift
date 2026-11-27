import Foundation
@testable import KodosiBridge
import KodosiKit
import Testing

@MainActor
@Test func `a change that went missing makes the chat ask its computer for the chat again`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }

    try await runtime.open()
    try runtime.send(#"{"snapshot":{"revision":2,"state":"idle","facts":{},"items":[]}}"#)
    try runtime.send(#"{"update":{"revision":4,"changes":[{"change":"item","item":{"id":"i5","at":5,"body":{"kind":"text","text":"Late"}}}]}}"#)

    #expect(runtime.chat.items.isEmpty)
    #expect(runtime.chat.linked == false)
    let closed = try await runtime.closed()
    #expect(closed == runtime.pipe)
    try await runtime.open()
    try runtime.send(#"{"snapshot":{"revision":4,"state":"idle","facts":{},"items":[{"id":"i5","at":5,"body":{"kind":"text","text":"Late"}}]}}"#)
    #expect(runtime.chat.items.map(\.id) == ["i5"])
}

@MainActor
@Test func `a question that the host still asks after a break of the pipe takes an answer again`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }
    let asking = #"""
    {"snapshot":{"revision":1,"state":"asking","facts":{},"items":[],"asks":[{"id":"a1","item":"i2","body":{"kind":"permission",
      "tool":{"name":"Bash","kind":"command","state":"asking","target":"make test"}}}]}}
    """#

    try await runtime.open()
    try runtime.send(asking)
    try runtime.chat.answer(#require(runtime.chat.question), .allow)
    #expect(runtime.chat.question == nil)
    try runtime.lose()
    try runtime.reopen()
    try runtime.send(asking)

    #expect(runtime.chat.question?.id == "a1")
}

@MainActor
@Test func `a message keeps the agents that it names apart from its words, and a hand-off says whose work it hands over`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }

    try await runtime.open()
    try runtime.send(#"""
    {"snapshot":{"revision":1,"state":"idle","facts":{},"items":[
      {"id":"i1","at":1,"body":{"kind":"person","text":"Ask @Timber Elk for the schema.","mentions":[{"name":"Timber Elk","agent":"copilot"}]}},
      {"id":"i2","at":2,"body":{"kind":"person","text":"","handoff":{"name":"Timber Elk"}}}],
     "queued":[{"id":"q1","text":"","handoff":{"name":"Timber Elk","agent":"copilot"}}],
     "peers":[{"terminal":"t2","name":"Timber Elk","agent":"copilot","place":"site"}]}}
    """#)

    let said = runtime.chat.items.compactMap { item -> ChatSaid? in
        if case let .person(said, _) = item.body {
            return said
        }
        return nil
    }
    #expect(said.map(\.text) == ["Ask @Timber Elk for the schema.", ""])
    #expect(said.map { $0.mentions.map(\.agent) } == [["copilot"], []])
    #expect(said.map { $0.handoff?.name } == [nil, "Timber Elk"])
    #expect(runtime.chat.queued.first?.said.handoff?.agent == "copilot")
    #expect(runtime.chat.peers.map(\.place) == ["site"])
    try runtime.send(#"{"update":{"revision":2,"changes":[{"change":"peers","peers":[]}]}}"#)
    #expect(runtime.chat.peers.isEmpty)
}

@MainActor
private final class ChatRuntime {
    let chat: TerminalChat
    private(set) var pipe: String?
    private let commands = CommandSink()
    private let store: CanvasStore
    private let log = ProjectCommandLog()
    private var sequence: UInt64 = 0

    init() {
        let log = log
        commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        commands.setAccount(userId: "user-1", epoch: 1)
        store = CanvasStore(commands: commands)
        chat = store.chat("terminal")
    }

    func open() async throws {
        let opening = try await log.next("document.open")
        #expect(opening["key"] as? String == "chat")
        try deliver("document.result", ["requestId": opening["requestId"] ?? "", "operation": "document.open"])
        pipe = try #require(opening["pipe"] as? String)
        try reopen()
    }

    func reopen() throws {
        sequence = 0
        try deliver("document.opened", ["pipe": pipe ?? ""])
    }

    func lose() throws {
        try deliver("document.lost", ["pipe": pipe ?? ""])
    }

    func closed() async throws -> String? {
        try await log.next("document.close")["pipe"] as? String
    }

    func send(_ frame: String) throws {
        let data = try JSONSerialization.data(withJSONObject: JSONSerialization.jsonObject(with: Data(frame.utf8)))
        try deliver("document.frame", ["pipe": pipe ?? "", "sequence": sequence, "more": false, "data": data.base64EncodedString()])
        sequence += 1
    }

    private func deliver(_ type: String, _ fields: [String: Any]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let event = try JSONDecoder().decode(RuntimeEvent.self, from: JSONSerialization.data(withJSONObject: payload))
        commands.receive(event)
        store.receive(event)
    }
}
