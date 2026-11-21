import Foundation
@testable import KodosiBridge
import KodosiKit
import Testing

@MainActor
@Test func `the chat applies each change in order`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }

    try await runtime.open()
    try runtime.send(#"{"snapshot":{"revision":4,"state":"idle","facts":{},"items":[]}}"#)
    try runtime.send(#"""
    {"update":{"revision":5,"changes":[
      {"change":"item","item":{"id":"i1","at":1,"body":{"kind":"person","text":"Fix the tax in app.py","by":{"id":"user-2","name":"Sam"}}}},
      {"change":"state","state":"working","now":{"kind":"thinking"}}]}}
    """#)
    try runtime.send(#"""
    {"update":{"revision":6,"changes":[
      {"change":"item","item":{"id":"i2","at":2,"body":{"kind":"tool","tool":{"name":"Edit","kind":"edit","state":"running","target":"app.py"}}}},
      {"change":"facts","facts":{"model":"sonnet","mode":"edits","context":{"tokens":650,"window":1000},"helpers":1}},
      {"change":"state","state":"working","now":{"kind":"tool","item":"i2"}}]}}
    """#)
    try runtime.send(#"{"update":{"revision":7,"changes":[{"change":"item","item":{"id":"i3","at":3,"body":{"kind":"text","text":"I made","live":true}}}]}}"#)
    try runtime.send(#"{"update":{"revision":8,"changes":[{"change":"text","id":"i3","text":" the fix."}]}}"#)
    try runtime.send(#"""
    {"update":{"revision":9,"changes":[
      {"change":"item","item":{"id":"i2","at":2,"body":{"kind":"tool","tool":{"name":"Edit","kind":"edit","state":"done","target":"app.py","ms":12}}}},
      {"change":"item","item":{"id":"i3","at":3,"body":{"kind":"text","text":"I made the fix."}}},
      {"change":"item","item":{"id":"i4","at":4,"body":{"kind":"turn","end":"done","ms":9017}}},
      {"change":"state","state":"idle"}]}}
    """#)

    let chat = try #require(runtime.chat.chat)
    #expect(chat.revision == 9)
    #expect(chat.items.map(\.id) == ["i1", "i2", "i3", "i4"])
    #expect(chat.items[1].body == .tool(ChatTool(name: "Edit", kind: .edit, state: .done, target: "app.py", ms: 12)))
    #expect(chat.items[2].body == .text("I made the fix.", live: false))
    #expect(chat.facts.model == "sonnet")
    #expect(chat.facts.mode == .edits)
    #expect(chat.state == .idle)
    #expect(chat.now == nil)
}

@MainActor
@Test func `after the pipe opens again, the chat that the host sends replaces the whole chat`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }

    try await runtime.open()
    try runtime.send(#"""
    {"snapshot":{"revision":2,"state":"working","facts":{"model":"opus"},"items":[
      {"id":"i1","at":1,"body":{"kind":"person","text":"Read notes.md"}}],
      "queued":[{"id":"q1","text":"Then run the tests"}]}}
    """#)
    try runtime.lose()
    #expect(runtime.chat.linked == false)
    try runtime.reopen()
    try runtime.send(#"{"update":{"revision":3,"changes":[{"change":"reset"}]}}"#)
    #expect(runtime.chat.items.map(\.id) == ["i1"])
    try runtime.send(#"""
    {"snapshot":{"revision":40,"state":"idle","facts":{"model":"sonnet"},"items":[
      {"id":"i9","at":9,"body":{"kind":"text","text":"Done."}}]}}
    """#)

    #expect(runtime.chat.linked)
    #expect(runtime.chat.items.map(\.id) == ["i9"])
    #expect(runtime.chat.queued.isEmpty)
    #expect(runtime.chat.facts.model == "sonnet")
    #expect(runtime.chat.state == .idle)
}

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
@Test func `the agent's list of its work follows the agent, and a new conversation has none`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }

    try await runtime.open()
    try runtime.send(#"""
    {"snapshot":{"revision":1,"state":"working","facts":{},"items":[],
      "tasks":[{"text":"Adding total discount","state":"working"},{"text":"Verifying discounted total","state":"pending"}],
      "asks":[{"id":"a1","body":{"kind":"plan","text":"1. Update total()","chooseAsks":true}}]}}
    """#)
    #expect(runtime.chat.tasks.map(\.state) == [.working, .pending])
    #expect(runtime.chat.asks.first?.body == .plan("1. Update total()", chooseAsks: true))
    try runtime.send(#"""
    {"update":{"revision":2,"changes":[{"change":"tasks","tasks":[
      {"text":"Adding total discount","state":"done"},{"text":"Verifying discounted total","state":"working"}]}]}}
    """#)
    #expect(runtime.chat.tasks == [
        ChatTask(text: "Adding total discount", state: .done),
        ChatTask(text: "Verifying discounted total", state: .working),
    ])
    try runtime.send(#"{"update":{"revision":3,"changes":[{"change":"reset"}]}}"#)

    #expect(runtime.chat.tasks.isEmpty)
    #expect(runtime.chat.asks.isEmpty)
}

@MainActor
@Test func `a message typed while a question waits answers the question with its words, and the first answer counts`() async throws {
    let runtime = ChatRuntime()
    let watching = Task { await runtime.chat.watch() }
    defer { watching.cancel() }

    try await runtime.open()
    try runtime.send(#"""
    {"snapshot":{"revision":1,"state":"asking","facts":{},"items":[],"asks":[{"id":"a1","item":"i2","body":{"kind":"permission",
      "tool":{"name":"Bash","kind":"command","state":"asking","target":"rm -rf build"},"always":{"kind":"rule","tool":"Bash","text":"rm -rf build"}}}]}}
    """#)
    let ask = try #require(runtime.chat.question)
    runtime.chat.draft = "  Keep the build folder  "
    runtime.chat.submit()
    runtime.chat.answer(ask, .allow)
    #expect(runtime.chat.question == nil)
    try runtime.send(#"{"update":{"revision":2,"changes":[{"change":"asks","asks":[]},{"change":"state","state":"working"}]}}"#)
    runtime.chat.draft = "Then run the tests"
    runtime.chat.submit()
    runtime.chat.stop()

    #expect(runtime.chat.answers.isEmpty)
    #expect(runtime.chat.draft.isEmpty)
    #expect(runtime.requests() == [
        #"{"answer":{"kind":"deny","text":"Keep the build folder"},"ask":"a1","do":"answer"}"#,
        #"{"do":"send","text":"Then run the tests"}"#,
        #"{"do":"stop"}"#,
    ])
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

    /// Each request with its keys in order, so that two requests compare as text.
    func requests() -> [String] {
        var requests: [String] = []
        while let command = log.take("document.send") {
            guard let data = (command["data"] as? String).flatMap({ Data(base64Encoded: $0) }),
                  let request = try? JSONSerialization.jsonObject(with: data),
                  let sorted = try? JSONSerialization.data(withJSONObject: request, options: [.sortedKeys, .withoutEscapingSlashes]),
                  let text = String(bytes: sorted, encoding: .utf8)
            else { continue }
            requests.append(text)
        }
        return requests
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
