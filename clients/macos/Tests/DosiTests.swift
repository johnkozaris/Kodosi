import Foundation
import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing

private func item(_ id: String, turn: String = "turn", _ body: [String: JSONValue]) -> JSONValue {
    .object(["id": .string(id), "turn": .string(turn), "at": .int(1_792_000_000_000), "body": .object(body)])
}

private func snapshot(state: String = "idle", conversation: String = "c1", items: [JSONValue], earlier: Bool = false,
                      remote: JSONValue = .null) -> [String: JSONValue]
{
    ["dosi": .object([
        "state": .string(state), "setup": .object(["agent": .string("claude"), "picks": .array([])]),
        "agents": .array([.object(["agent": .string("claude"), "installed": .bool(true)])]),
        "instructionsEdited": .bool(false), "folder": .string("/tmp/dosi"), "shared": .bool(true), "remote": remote,
        "conversation": .object(["id": .string(conversation), "startedAt": .int(1_792_000_000_000)]),
        "items": .array(items), "earlier": .bool(earlier), "queued": .array([]),
    ])]
}

private func person(_ id: String, _ text: String) -> JSONValue {
    item(id, ["kind": .string("person"), "text": .string(text)])
}

private func text(_ id: String, _ text: String, live: Bool = false) -> JSONValue {
    item(id, ["kind": .string("text"), "text": .string(text), "live": .bool(live)])
}

@Test @MainActor func `a snapshot keeps earlier pages and a new conversation starts clean`() async throws {
    let commands = CommandSink()
    var sent: [[String: JSONValue]] = []
    commands.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    let dosi = DosiStore(commands: commands)
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(items: [person("b", "And now?"), text("c", "Two need you.")],
                                                                    earlier: true)))
    #expect(dosi.items.map(\.id) == ["b", "c"])
    #expect(dosi.earlier)
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": text("d", "Wor", live: true)]))
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": text("d", "Working on it.")]))
    #expect(dosi.items.map(\.id) == ["b", "c", "d"])
    #expect(dosi.items.last?.body == .text("Working on it.", live: false))

    let loading = Task { try await dosi.loadEarlier() }
    for _ in 0 ..< 200 where !sent.contains(where: { $0["type"] == .string("dosi.history") }) {
        await Task.yield()
    }
    let history = try #require(sent.last { $0["type"] == .string("dosi.history") })
    #expect(history["before"] == .string("b"))
    _ = try commands.receive(runtimeEvent("dosi.result", fields: [
        "requestId": #require(history["requestId"]), "operation": .string("dosi.history"),
        "page": .object(["items": .array([person("a", "What needs me?")]), "earlier": .bool(false)]),
    ]))
    try await loading.value
    #expect(dosi.items.map(\.id) == ["a", "b", "c", "d"])
    #expect(!dosi.earlier)

    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(items: [text("c", "Two need you."), text("d", "Done.")])))
    #expect(dosi.items.map(\.id) == ["a", "b", "c", "d"])
    #expect(dosi.items.last?.body == .text("Done.", live: false))

    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(conversation: "c2", items: [])))
    #expect(dosi.items.isEmpty)
}

@Test @MainActor func `a terminal finds its question and the face follows the turn`() throws {
    let dosi = DosiStore(commands: CommandSink())
    let question = item("q", [
        "kind": .string("question"), "question": .string("Continue?"), "options": .array([.string("y"), .string("n")]),
        "multiple": .bool(false), "terminal": .string("Terminal"), "answer": .null,
    ])
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(items: [person("a", "Watch it"), question])))
    let terminal = try runtimeSession()
    #expect(dosi.question(forTerminal: terminal)?.0.id == "q")
    #expect(dosi.mood() == .asking)
    let reading = item("t", ["kind": .string("tool"), "action": .string("read"),
                             "target": .object(["kind": .string("terminal"), "value": .string("api")]),
                             "text": .null, "status": .string("running"), "output": .null])
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(state: "working", items: [person("a", "Watch it"), reading])))
    #expect(dosi.question(forTerminal: terminal) == nil)
    #expect(dosi.mood() == .reading)
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(state: "working", items: [person("a", "Watch it")],
                                                                    remote: .object(["computer": .string("Mac Studio"), "since": .int(1)]))))
    #expect(dosi.mood() == .thinking)
    #expect(DosiTool.Action.allCases.allSatisfy { DosiWords.deeds[$0] != nil })
}

@Test @MainActor func `a reply from before the start is read and a new one is not`() throws {
    let dosi = DosiStore(commands: CommandSink())
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(items: [person("a", "Hi"), text("b", "Hello.")])))
    #expect(!dosi.unseen)
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": item("c", turn: "two", ["kind": .string("person"), "text": .string("And?")])]))
    #expect(!dosi.unseen)
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": item("d", turn: "two", ["kind": .string("text"), "text": .string("Done."),
                                                                                        "live": .bool(false)])]))
    #expect(dosi.unseen)
    dosi.see()
    #expect(!dosi.unseen)
}

@Test @MainActor func `a message to Dosi names the terminal that the person looks at`() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "DosiContext"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    app.workbench.section = .sessions
    app.workbench.selectedSessionId = "01992e43-53db-7040-8e02-f6ebf4149b28"
    app.dosi.draft = "  What does it need?  "
    app.sendToDosi()
    #expect(app.dosi.draft.isEmpty)
    for _ in 0 ..< 200 where !sent.contains(where: { $0["type"] == .string("dosi.send") }) {
        await Task.yield()
    }
    let send = try #require(sent.last { $0["type"] == .string("dosi.send") })
    #expect(send["text"] == .string("What does it need?"))
    #expect(send["context"] == .object(["terminal": .string("01992e43-53db-7040-8e02-f6ebf4149b28"), "room": .null]))
}

@Test @MainActor func `in the thread Dosi thinks until its turn shows something`() throws {
    let dosi = DosiStore(commands: CommandSink())
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(state: "working", items: [person("a", "What needs me?")])))
    #expect(dosi.pending)
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": text("b", "Two", live: true)]))
    #expect(!dosi.pending)
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": text("b", "Two need you.")]))
    #expect(dosi.pending)
    try dosi.receive(runtimeEvent("dosi.item", fields: ["item": item("q", [
        "kind": .string("question"), "question": .string("Continue?"), "options": .array([.string("y"), .string("n")]),
        "multiple": .bool(false), "terminal": .null, "answer": .null,
    ])]))
    #expect(!dosi.pending)
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(state: "working", items: [person("a", "Watch it")],
                                                                    remote: .object(["computer": .string("Mac Studio"), "since": .int(1)]))))
    #expect(!dosi.pending)
    try dosi.receive(runtimeEvent("dosi.snapshot", fields: snapshot(items: [person("a", "What needs me?")])))
    #expect(!dosi.pending)
}

@Test @MainActor func `the sidebar keeps a width for the list and one for Dosi`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "DosiWidth"))
    let workbench = WorkbenchState(defaults: defaults)
    #expect(workbench.shownSidebarWidth == Metrics.sidebarWidth)
    workbench.resizeSidebar(to: 900)
    #expect(workbench.shownSidebarWidth == WorkbenchState.sidebarWidths.upperBound)
    workbench.toggleDosi()
    #expect(workbench.dosiOpen && workbench.shownSidebarWidth == Metrics.dosiWidth)
    workbench.resizeSidebar(to: 500)
    workbench.sidebarCollapsed = true
    #expect(workbench.shownSidebarWidth == Metrics.sidebarRailWidth)
    workbench.toggleDosi()
    #expect(!workbench.sidebarCollapsed && workbench.dosiOpen)
    let restored = WorkbenchState(defaults: defaults)
    #expect(restored.dosiOpen && restored.shownSidebarWidth == 500)
}

@Test @MainActor func `turning Dosi off closes it and keeps it closed`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "DosiOff"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.openDosi()
    workbench.dosiOn = false
    #expect(!workbench.dosiOpen)
    workbench.toggleDosi()
    #expect(!workbench.dosiOpen && workbench.shownSidebarWidth == Metrics.sidebarWidth)
    let restored = WorkbenchState(defaults: defaults)
    #expect(!restored.dosiOn && !restored.dosiOpen)
}

@Test func `the thread groups steps of a turn and marks long gaps`() throws {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = try #require(TimeZone(identifier: "UTC"))
    let now = try #require(calendar.date(from: DateComponents(year: 2026, month: 10, day: 18, hour: 15)))
    func at(minutes: Double) -> Int64 {
        Int64((now.timeIntervalSince1970 + minutes * 60) * 1000)
    }
    let look = DosiTool(action: .look, target: DosiTarget(kind: .terminals, value: ""), status: .done)
    let read = DosiTool(action: .read, target: DosiTarget(kind: .terminal, value: "api"), status: .done)
    let typed = DosiTool(action: .type, target: DosiTarget(kind: .terminal, value: "build"), text: "just test", status: .done)
    let items = [
        DosiItem(id: "a", turn: "1", at: at(minutes: -1500), body: .person("What needs me?")),
        DosiItem(id: "b", turn: "1", at: at(minutes: -1499), body: .tool(look)),
        DosiItem(id: "c", turn: "1", at: at(minutes: -1499), body: .tool(read)),
        DosiItem(id: "d", turn: "1", at: at(minutes: -1498), body: .text("Two need you.", live: false)),
        DosiItem(id: "x", turn: "x", at: at(minutes: -120), body: .person("Anything new?")),
        DosiItem(id: "e", turn: "2", at: at(minutes: -5), body: .person("Run the tests.")),
        DosiItem(id: "f", turn: "2", at: at(minutes: -4), body: .tool(typed)),
    ]
    let rows = DosiRow.rows(items, now: now, calendar: calendar)
    #expect(rows.map(\.id) == ["divider.a", "a", "b", "d", "divider.x", "x", "e", "f"])
    #expect(rows[0].kind == .divider("yesterday") && rows[4].kind == .divider("this afternoon"))
    #expect(rows[2].kind == .tools([items[1], items[2]]) && rows[2].startsTurn && !rows[3].startsTurn)
    #expect(rows[7].kind == .item(items[6]))
    #expect(DosiWords.summary([look, read]) == "worked on the terminals, api")
    #expect(DosiWords.receipt(typed) == "typed just test · build")
    let typing = DosiTool(action: .type, target: DosiTarget(kind: .terminal, value: "build"), text: "just test", status: .running)
    #expect(DosiWords.receipt(typing) == "typing just test · build" && DosiWords.said(typed) == "typed just test")
}

@Test @MainActor func `a step in a terminal touches it once and the card follows the step`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "dosi-touch"))
    let app = try AppDependencies(defaults: defaults, storageBootstrap: RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false))
    app.sessions = try [runtimeSession(kind: "local")]
    let id = app.sessions[0].id
    func step(_ item: String, _ action: DosiTool.Action, _ status: DosiTool.Status) -> DosiItem {
        let tool = DosiTool(action: action, target: DosiTarget(kind: .terminal, value: "terminal"), text: "y", status: status)
        return DosiItem(id: item, turn: "1", at: 0, body: .tool(tool))
    }
    app.touch(by: step("a", .type, .running))
    app.touch(by: step("a", .type, .done))
    #expect(app.dosiTouches[id]?.count == 1 && app.dosiTouches[id]?.tool?.status == .done && app.dosiTouches[id]?.shows == true)
    app.touch(by: step("b", .read, .running))
    app.touch(by: step("a", .type, .done))
    #expect(app.dosiTouches[id]?.count == 2 && app.dosiTouches[id]?.tool?.action == .read && app.dosiTouches[id]?.shows == false)
}

@Test func `the shelf shows only whole chips and a dropdown for the rest`() {
    let more: (Int) -> CGFloat = { _ in 30 }
    #expect(DosiShelfFit.shown([100, 100, 100], in: 308, more: more) == 3)
    #expect(DosiShelfFit.shown([100, 100, 100, 100], in: 341, more: more) == 2)
    #expect(DosiShelfFit.shown([100, 100, 100, 100], in: 342, more: more) == 3)
    #expect(DosiShelfFit.shown([100, 100], in: 20, more: more) == 0)
}
