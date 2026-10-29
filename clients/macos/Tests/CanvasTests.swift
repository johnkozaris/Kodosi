import CoreGraphics
import Foundation
import KodosiBridge
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

private func terminal(_ id: String = "01992e43-53db-7040-8e02-f6ebf4149b28", canvas: [String: JSONValue]?) throws -> RuntimeSession {
    var fields: [String: JSONValue] = [
        "id": .string(id), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("checkout"), "program": .string("claude"),
        "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
        "connectionState": .string("local"), "sharedWith": .array([]),
    ]
    fields["canvas"] = canvas.map(JSONValue.object)
    return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
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

@Test func `a point of the view becomes a point of the browser`() {
    let wide = CGSize(width: 1000, height: 500)
    let picture = CGSize(width: 1280, height: 720)
    #expect(BrowserFeed.point(CGPoint(x: 500, y: 250), in: wide, picture: picture, browser: picture) == .init(x: 640, y: 360))
    #expect(BrowserFeed.point(CGPoint(x: 10, y: 250), in: wide, picture: picture, browser: picture) == nil)
    let sharp = CGSize(width: 2560, height: 1440)
    #expect(BrowserFeed.point(CGPoint(x: 500, y: 250), in: wide, picture: sharp, browser: picture) == .init(x: 640, y: 360))
    let narrow = CGSize(width: 451, height: 690)
    let page = CGSize(width: 451, height: 690)
    let window = CGSize(width: 500, height: 690)
    #expect(BrowserFeed.point(CGPoint(x: 75.8, y: 103.3), in: narrow, picture: window, browser: page) == .init(x: 84, y: 77))
}

@Test func `the agent's point of the browser lands where the person sees it`() {
    let wide = CGSize(width: 1000, height: 500)
    let picture = CGSize(width: 1280, height: 720)
    let middle = BrowserFeed.viewPoint(.init(x: 640, y: 360), in: wide, picture: picture, browser: picture)
    #expect(middle == CGPoint(x: 500, y: 250))
    let sharp = CGSize(width: 2560, height: 1440)
    #expect(BrowserFeed.viewPoint(.init(x: 640, y: 360), in: wide, picture: sharp, browser: picture) == CGPoint(x: 500, y: 250))
    #expect(BrowserFeed.viewPoint(.init(x: 1, y: 1), in: .zero, picture: picture, browser: picture) == nil)
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

@Test @MainActor func `a canvas opens by itself once, for a terminal alone on the stage`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageCanvasArrival"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount(nil)
    let empty = try terminal(canvas: nil)
    _ = workbench.reconcile([empty])
    workbench.showSession(empty.id)
    let first = try terminal(canvas: ["sequence": .uint(1), "items": .array([item("plan.md", changed: 1)])])
    _ = workbench.reconcile([first])
    #expect(workbench.canvasSessionId == first.id)
    workbench.hideCanvas()
    _ = workbench.reconcile([empty])
    _ = workbench.reconcile([first])
    #expect(workbench.canvasSessionId == nil)
}

@Test func `ask about this types one line for the person to finish`() throws {
    let session = try terminal(canvas: ["sequence": .uint(1), "items": .array([item("plan.md", changed: 1)])])
    let shown = try #require(session.canvas?.shown.first)
    #expect(CanvasQuestion.about(shown, words: nil) == "About /tmp/work/plan.md: ")
    #expect(CanvasQuestion.about(shown, words: " round\neach\u{1B}[2J line\r") == "About /tmp/work/plan.md: \"round each [2J line\" ")
    #expect(CanvasQuestion.about(shown, words: "  ") == "About /tmp/work/plan.md: ")
}
