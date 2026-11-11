import CoreGraphics
import Foundation
@testable import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing

private func item(_ name: String, changed: UInt64, revision: UInt64 = 1, missing: Bool = false) -> JSONValue {
    var fields: [String: JSONValue] = [
        "key": .string("/tmp/work/\(name)"), "name": .string(name), "kind": .string(name.hasSuffix(".pdf") ? "pdf" : "page"),
        "revision": .uint(revision), "bytes": .uint(120), "changed": .uint(changed),
    ]
    if missing {
        fields["missing"] = .bool(true)
    }
    return .object(fields)
}

private func terminalFields(_ id: String = "01992e43-53db-7040-8e02-f6ebf4149b28", canvas: [String: JSONValue]?) -> [String: JSONValue] {
    var fields: [String: JSONValue] = [
        "id": .string(id), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("checkout"), "program": .string("claude"),
        "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
        "connectionState": .string("local"), "sharedWith": .array([]),
    ]
    fields["canvas"] = canvas.map(JSONValue.object)
    return fields
}

private func terminal(_ id: String = "01992e43-53db-7040-8e02-f6ebf4149b28", canvas: [String: JSONValue]?) throws -> RuntimeSession {
    try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(terminalFields(id, canvas: canvas)))
}

private func asleepBrowser(changed: UInt64) -> JSONValue {
    .object([
        "state": .string("asleep"), "changed": .uint(changed), "width": .uint(1280), "height": .uint(720),
        "tabs": .array([.object([
            "id": .string("1"), "title": .string("Doodles"), "url": .string("https://doodles.google/"), "active": .bool(true),
        ])]),
    ])
}

@Test func `a canvas lists its items, newest first, and then the browser`() throws {
    let session = try terminal(canvas: [
        "sequence": .uint(9),
        "items": .array([item("compare.html", changed: 4), item("plan.pdf", changed: 7, missing: true)]),
        "browser": .object(["state": .string("asleep"), "changed": .uint(2), "width": .uint(1280), "height": .uint(720)]),
    ])
    let canvas = try #require(session.canvas)
    #expect(canvas.shown.map(\.key) == ["/tmp/work/compare.html", "/tmp/work/plan.pdf", CanvasShown.browserKey])
    #expect(canvas.newest?.key == "/tmp/work/plan.pdf")
    #expect(canvas.shown[1].missing)
    #expect(canvas.browser?.tabs.isEmpty == true)
    #expect(try terminal(canvas: nil).canvas == nil)
    #expect(try terminal(canvas: ["sequence": .uint(1)]).canvas?.shown.isEmpty == true)
}

@Test @MainActor func `a canvas shows news only for what changed after this device saw it`() throws {
    let store = CanvasStore(commands: CommandSink())
    let first = try terminal(canvas: ["sequence": .uint(4), "items": .array([item("compare.html", changed: 4)])])
    store.observe([first])
    #expect(!store.hasNews(first))
    let changed = try terminal(canvas: [
        "sequence": .uint(6), "items": .array([item("plan.md", changed: 6), item("compare.html", changed: 4)]),
    ])
    store.observe([changed])
    #expect(store.hasNews(changed))
    let newest = try #require(store.current(changed))
    #expect(newest.key == "/tmp/work/plan.md")
    store.saw(newest, in: changed.id)
    #expect(!store.hasNews(changed))
    store.pick("/tmp/work/compare.html", in: changed.id)
    let edited = try terminal(canvas: [
        "sequence": .uint(8), "items": .array([item("plan.md", changed: 8, revision: 2), item("compare.html", changed: 4)]),
    ])
    store.observe([edited])
    #expect(store.current(edited)?.key == "/tmp/work/compare.html")
    #expect(try store.isNew(#require(edited.canvas?.shown.first), in: edited.id))
    let removed = try terminal(canvas: ["sequence": .uint(9), "items": .array([item("plan.md", changed: 8, revision: 2)])])
    store.observe([removed])
    #expect(store.current(removed)?.key == "/tmp/work/plan.md")
}

@Test @MainActor func `a canvas opens once at each show of its agent, and not at a show from before`() throws {
    let store = CanvasStore(commands: CommandSink(), started: 6)
    let shown = { (at: UInt64, key: String) in
        try terminal(canvas: [
            "sequence": .uint(at), "items": .array([item("plan.md", changed: 5), item("compare.html", changed: 4)]),
            "open": .object(["key": .string("/tmp/work/\(key)"), "at": .uint(at)]),
        ])
    }
    let before = try shown(6, "compare.html")
    store.observe([before])
    #expect(!store.openShown(before))
    let again = try shown(7, "compare.html")
    store.observe([again])
    #expect(store.showsNew(again))
    #expect(store.openShown(again))
    #expect(store.current(again)?.key == "/tmp/work/compare.html")
    #expect(!store.openShown(again))
    let next = try shown(8, "plan.md")
    store.observe([next])
    store.close(.files, in: next)
    #expect(!store.openShown(next))
    let gone = try shown(9, "gone.md")
    store.observe([gone])
    #expect(!store.showsNew(gone))
}

@Test @MainActor func `a terminal whose canvas comes later opens only a show from after the start`() throws {
    let store = CanvasStore(commands: CommandSink(), started: 100)
    let shown = { (at: UInt64) in
        try terminal(canvas: [
            "sequence": .uint(at), "items": .array([item("plan.md", changed: 5)]),
            "open": .object(["key": .string("/tmp/work/plan.md"), "at": .uint(at)]),
        ])
    }
    try store.observe([terminal(canvas: nil)])
    let old = try shown(90)
    store.observe([old])
    #expect(!store.openShown(old))
    let fresh = try shown(120)
    try store.observe([terminal(canvas: nil)])
    store.observe([fresh])
    #expect(store.openShown(fresh))
    #expect(!store.openShown(fresh))
}

@Test @MainActor func `a canvas that a person closed on this device opens again at its newest part only`() throws {
    let store = CanvasStore(commands: CommandSink())
    let both = try terminal(canvas: [
        "sequence": .uint(5), "items": .array([item("plan.md", changed: 3)]), "browser": asleepBrowser(changed: 5),
    ])
    store.observe([both])
    store.close(.files, in: both)
    store.open(both)
    #expect(store.sections(both) == [.browser])
    store.close(.browser, in: both)
    #expect(!store.showsSomething(both) && store.current(both) == nil)
    store.open(both)
    #expect(store.sections(both) == [.browser] && store.current(both)?.key == CanvasShown.browserKey)
}

@Test func `a point of the view becomes a point of the browser`() {
    let wide = CGSize(width: 1000, height: 500)
    let browser = CGSize(width: 1280, height: 720)
    #expect(BrowserFeed.point(CGPoint(x: 500, y: 250), in: wide, browser: browser) == .init(x: 640, y: 360))
    #expect(BrowserFeed.point(CGPoint(x: 10, y: 250), in: wide, browser: browser) == nil)
    let narrow = CGSize(width: 451, height: 690)
    #expect(BrowserFeed.point(CGPoint(x: 75.8, y: 103.3), in: narrow, browser: narrow) == .init(x: 75.8, y: 103.3))
}

@Test func `the agent's point of the browser lands where the person sees it`() {
    let wide = CGSize(width: 1000, height: 500)
    let browser = CGSize(width: 1280, height: 720)
    #expect(BrowserFeed.viewPoint(CGPoint(x: 640, y: 360), in: wide, browser: browser) == CGPoint(x: 500, y: 250))
    #expect(BrowserFeed.viewPoint(CGPoint(x: 1, y: 1), in: .zero, browser: browser) == nil)
}

@Test @MainActor func `a document that loses its pipe closes it in the runtime`() async throws {
    let commands = CommandSink()
    var sent: [[String: JSONValue]] = []
    commands.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(command)
        }
        return 0
    }
    let store = CanvasStore(commands: commands)
    let link = store.browser(of: "01992e43-53db-7040-8e02-f6ebf4149b28")
    var lost = 0
    link.onLost = { lost += 1 }
    let opening = Task { try await link.open() }
    while link.pipe == nil {
        await Task.yield()
    }
    let pipe = try #require(link.pipe)
    try store.receive(runtimeEvent("document.frame", fields: [
        "pipe": .string(pipe), "sequence": .uint(3), "more": .bool(false), "data": .string("AAAA"),
    ]))
    #expect(lost == 1 && link.pipe == nil)
    #expect(sent.last?["type"] == .string("document.close") && sent.last?["pipe"] == .string(pipe))
    opening.cancel()
    _ = await opening.result
    #expect(sent.count(where: { $0["type"] == .string("document.close") }) == 1)
}

@Test func `a frame of the browser splits at its start codes`() {
    let frame = Data([0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 4, 0])
    #expect(H264Samples.units(of: frame).map(Array.init) == [[0x67, 1, 2], [0x68, 3], [0x65, 4, 0]])
}

@Test func `a canvas and its terminal share the stage, or the canvas fills it`() {
    let even = CanvasPair(width: 1200, chosen: nil)
    #expect(even.panel == 600 && !even.narrow)
    #expect(CanvasPair(width: 1200, chosen: 1000).panel == 820)
    #expect(CanvasPair(width: 1200, chosen: 100).panel == CanvasPair.narrowest)
    #expect(CanvasPair(width: 700, chosen: nil).narrow)
}

@Test func `the terminal of a canvas that fills the stage zooms out to its corner`() {
    #expect(CanvasPair.zoom(tile: CGSize(width: 600, height: 700), stage: CGSize(width: 1200, height: 700)) == 0.4)
    #expect(CanvasPair.zoom(tile: CGSize(width: 1600, height: 900), stage: CGSize(width: 1600, height: 900)) == 0.2)
    #expect(CanvasPair.zoom(tile: CGSize(width: 300, height: 200), stage: CGSize(width: 2000, height: 1600)) == 0.45)
    #expect(CanvasPair.zoom(tile: .zero, stage: CGSize(width: 1200, height: 700)) == 1)
}

@Test @MainActor func `a canvas opens beside its terminal alone, which stays alone when it hides`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvas"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount(nil)
    let plain = try terminal("01992e43-53db-7040-8e02-f6ebf4149b29", canvas: nil)
    let shown = try terminal(canvas: ["sequence": .uint(1), "items": .array([item("plan.md", changed: 1)])])
    _ = workbench.reconcile([shown, plain])
    workbench.showSession(shown.id)
    workbench.showSession(plain.id)
    #expect(workbench.focusedSessionId == nil)
    workbench.openCanvas(shown.id)
    #expect(workbench.canvasSessionId == shown.id && workbench.focusedSessionId == shown.id)
    workbench.showSession(plain.id)
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == plain.id)
    workbench.toggleFocus(plain.id)
    workbench.openCanvas(shown.id)
    workbench.hideCanvas()
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == shown.id)
    workbench.openCanvas(shown.id)
    workbench.toggleFocus(shown.id)
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == nil)
    workbench.openCanvas(shown.id)
    _ = try workbench.reconcile([terminal(canvas: nil), plain])
    #expect(workbench.canvasSessionId == nil)
}

@Test @MainActor func `a canvas left open is open again when the person comes back to its terminal`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvasBack"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount(nil)
    let plain = try terminal("01992e43-53db-7040-8e02-f6ebf4149b29", canvas: nil)
    let shown = try terminal(canvas: ["sequence": .uint(1), "items": .array([item("plan.md", changed: 1)])])
    _ = workbench.reconcile([shown, plain])
    workbench.showSession(shown.id)
    workbench.showSession(plain.id)
    workbench.openCanvas(shown.id)
    workbench.showSession(plain.id)
    #expect(workbench.canvasSessionId == nil)
    workbench.showSession(shown.id)
    #expect(workbench.canvasSessionId == shown.id)
    workbench.selectAdjacentSession(offset: 1)
    workbench.toggleFocus(plain.id)
    workbench.toggleFocus(shown.id)
    #expect(workbench.canvasSessionId == shown.id)
    workbench.hideCanvas()
    workbench.showSession(plain.id)
    workbench.showSession(shown.id)
    #expect(workbench.canvasSessionId == nil)
}

@Test @MainActor func `a canvas that a person opens before it has anything shows when it comes`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvasLater"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount(nil)
    let empty = try terminal(canvas: nil)
    let other = try terminal("01992e43-53db-7040-8e02-f6ebf4149b29", canvas: nil)
    _ = workbench.reconcile([empty, other])
    workbench.showSession(empty.id)
    workbench.showSession(other.id)
    workbench.openCanvas(empty.id)
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == empty.id)
    let browsing = try terminal(canvas: [
        "sequence": .uint(2),
        "browser": .object(["state": .string("starting"), "changed": .uint(2), "width": .uint(1280), "height": .uint(720)]),
    ])
    _ = workbench.reconcile([browsing, other])
    #expect(workbench.canvasSessionId == browsing.id)
}

@Test @MainActor func `the stage shows a canvas only with something open on this device, and opening it shows its newest part`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvasClosed"))
    let store = CanvasStore(commands: CommandSink())
    let workbench = WorkbenchState(defaults: defaults, canvas: store)
    workbench.switchAccount(nil)
    let files = try terminal("01992e43-53db-7040-8e02-f6ebf4149b29", canvas: [
        "sequence": .uint(2), "items": .array([item("plan.md", changed: 1)]), "browser": asleepBrowser(changed: 2),
    ])
    let asleep = try terminal(canvas: ["sequence": .uint(1), "browser": asleepBrowser(changed: 1)])
    store.observe([files, asleep])
    _ = workbench.reconcile([files, asleep])
    workbench.showSession(files.id)
    workbench.showSession(asleep.id)
    store.close(.browser, in: files)
    store.close(.browser, in: asleep)
    workbench.openCanvas(files.id)
    #expect(workbench.canvasSessionId == files.id)
    workbench.showSession(asleep.id)
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == asleep.id)
    workbench.openCanvas(asleep.id)
    #expect(workbench.canvasSessionId == asleep.id && store.current(asleep)?.key == CanvasShown.browserKey)
    workbench.openCanvas(files.id)
    #expect(workbench.canvasSessionId == files.id && store.sections(files) == [.files])
    let emptied = try terminal(files.id, canvas: ["sequence": .uint(3), "browser": asleepBrowser(changed: 2)])
    store.observe([emptied, asleep])
    _ = workbench.reconcile([emptied, asleep])
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == files.id)
}

@Test @MainActor func `a file added to a canvas closed on this device opens it at the file when it comes, and the browser stays closed`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvasAddFile"))
    let store = CanvasStore(commands: CommandSink())
    let workbench = WorkbenchState(defaults: defaults, canvas: store)
    workbench.switchAccount(nil)
    let asleep = try terminal(canvas: ["sequence": .uint(1), "browser": asleepBrowser(changed: 1)])
    store.observe([asleep])
    _ = workbench.reconcile([asleep])
    workbench.showSession(asleep.id)
    store.close(.browser, in: asleep)
    workbench.openCanvas(asleep.id, at: .files)
    #expect(workbench.canvasSessionId == nil && workbench.focusedSessionId == asleep.id)
    let added = try terminal(canvas: [
        "sequence": .uint(2), "items": .array([item("plan.md", changed: 2)]), "browser": asleepBrowser(changed: 1),
    ])
    store.observe([added])
    _ = workbench.reconcile([added])
    #expect(workbench.canvasSessionId == added.id && store.current(added)?.key == "/tmp/work/plan.md")
    #expect(store.closedSections(added) == [.browser])
}

@Test @MainActor func `the stage opens a canvas that comes later only at a show from after the start`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvasShown"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    app.commandSink.transport = { _ in 0 }
    app.windowShown = { true }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    var fields = terminalFields(canvas: nil)
    func snapshot(_ canvas: [String: JSONValue]?) throws {
        fields["canvas"] = canvas.map(JSONValue.object)
        try app.receive(runtimeEvent("sessions.snapshot", epoch: 0, userId: nil, fields: ["sessions": .array([.object(fields)])]))
    }
    try snapshot(nil)
    let id = try #require(app.sessions.first?.id)
    app.workbench.showSession(id)
    try snapshot([
        "sequence": .uint(1), "items": .array([item("plan.md", changed: 1)]),
        "open": .object(["key": .string("/tmp/work/plan.md"), "at": .uint(1)]),
    ])
    #expect(app.workbench.canvasSessionId == nil)
    let later = UInt64(Date.now.timeIntervalSince1970 * 1000) + 60000
    try snapshot([
        "sequence": .uint(2), "items": .array([item("plan.md", changed: 1)]),
        "browser": .object(["state": .string("starting"), "changed": .uint(2), "width": .uint(1280), "height": .uint(720)]),
        "open": .object(["key": .string(CanvasShown.browserKey), "at": .uint(later)]),
    ])
    #expect(app.workbench.canvasSessionId == id)
    let session = try #require(app.sessions.first)
    #expect(app.canvas.current(session)?.key == CanvasShown.browserKey)
}

@Test func `ask about this types one line for the person to finish`() throws {
    let session = try terminal(canvas: ["sequence": .uint(1), "items": .array([item("plan.md", changed: 1)])])
    let shown = try #require(session.canvas?.shown.first)
    #expect(CanvasQuestion.about(shown, words: nil) == "About /tmp/work/plan.md: ")
    #expect(CanvasQuestion.about(shown, words: " round\neach\u{1B}[2J line\r") == "About /tmp/work/plan.md: \"round each [2J line\" ")
    #expect(CanvasQuestion.about(shown, words: "  ") == "About /tmp/work/plan.md: ")
}

@Test func `the address offers the pages that fit each word, those that start with them first`() {
    var history = BrowserHistory()
    let start = Date(timeIntervalSince1970: 1_000_000)
    history.visit("https://news.example.com/git", title: "Git news", at: start)
    history.visit("https://github.com/johnkozaris/Kodosi", title: "Kodosi", at: start.addingTimeInterval(1))
    history.visit("https://docs.example.org/guide", title: "Git guide", at: start.addingTimeInterval(2))
    history.visit("https://docs.example.org/guide", title: "Git guide", at: start.addingTimeInterval(3))
    #expect(history.matches("git").map(\.address) == [
        "https://github.com/johnkozaris/Kodosi", "https://docs.example.org/guide", "https://news.example.com/git",
    ])
    #expect(history.matches("git guide").map(\.address) == ["https://docs.example.org/guide"])
    #expect(history.matches("https://www.github").map(\.address) == ["https://github.com/johnkozaris/Kodosi"])
    #expect(history.matches("  ").isEmpty)
}

@Test func `a page is kept once with its newest title, and only web pages are kept`() throws {
    var history = BrowserHistory()
    let start = Date(timeIntervalSince1970: 1_000_000)
    history.visit("about:blank", title: "", at: start)
    history.visit("https://example.com/", title: "", at: start)
    history.name("https://example.com/", title: "Example")
    history.visit("https://example.com/", title: "", at: start.addingTimeInterval(1))
    let page = try #require(history.pages.first)
    #expect(history.pages.count == 1)
    #expect((page.title, page.visits) == ("Example", 2))
    let kept = try #require(BrowserHistory(rawValue: history.rawValue))
    #expect(kept == history)
}

@Test @MainActor func `settings of the browser read its status while no download runs`() async throws {
    let commands = CommandSink()
    var request: String?
    commands.transport = { data in
        request = (try? JSONDecoder().decode([String: JSONValue].self, from: data))?["requestId"]?.stringValue
        return 0
    }
    let store = CanvasStore(commands: commands)
    let reading = Task { try await store.browserStatus() }
    while request == nil {
        await Task.yield()
    }
    try commands.receive(runtimeEvent("browser.result", fields: [
        "requestId": .string(#require(request)), "operation": .string("browser.status"),
        "sources": .array([.object(["name": .string("Chrome"), "browser": .string("chrome"), "protected": .bool(false)])]),
        "sites": .uint(0),
    ]))
    let status = try await reading.value
    #expect(status == BrowserStatus(sources: [.init(name: "Chrome", browser: "chrome", protected: false)], sites: 0, engine: nil,
                                    getting: nil))
}
