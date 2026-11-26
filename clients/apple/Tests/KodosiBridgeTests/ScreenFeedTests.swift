import CoreGraphics
import Foundation
@testable import KodosiBridge
import Testing

@MainActor
@Test func `a picture of a part of the screen shows where that part is`() async throws {
    let runtime = FeedRuntime()
    let shown = Shown()
    runtime.screen.show(on: NSObject(), shown)
    let watching = Task { await runtime.screen.watch() }
    defer { watching.cancel() }
    let pipe = try await runtime.open()

    let part = CGRect(x: 100, y: 50, width: 320, height: 180)
    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    try runtime.send(FeedRuntime.frame(2, region: part), on: pipe)
    #expect(try await runtime.messages(2) == ["ack", "ack"])
    await eventually { shown.count == 2 }
    #expect(shown.regions == [nil, part])
}

@MainActor
@Test func `a zoom keeps the point under the fingers in place and asks the computer for that part`() async throws {
    let runtime = FeedRuntime()
    let watching = Task { await runtime.screen.watch() }
    defer { watching.cancel() }
    let pipe = try await runtime.open()
    try runtime.say([
        "source": ["kind": "display", "id": 1], "width": 1600, "height": 1000, "access": ["screen": true, "input": true],
        "control": true,
    ], on: pipe)
    await eventually { runtime.screen.size.width > 0 }

    let view = CGSize(width: 800, height: 500)
    runtime.screen.viewSize = view
    let fingers = CGPoint(x: 600, y: 120)
    let before = try #require(runtime.screen.placement(in: view)?.sourcePoint(fingers, in: view))
    runtime.screen.zoom(by: 2, at: fingers, in: view)
    let placement = try #require(runtime.screen.placement(in: view))
    let after = try #require(placement.sourcePoint(fingers, in: view))

    #expect(abs(after.x - before.x) < 0.001 && abs(after.y - before.y) < 0.001)
    #expect(CGRect(x: 0, y: 0, width: 1600, height: 1000).contains(placement.look))
    let look = try await runtime.next("look")
    let region = try #require(look["region"] as? [String: Double])
    let asked = CGRect(x: region["x"] ?? 0, y: region["y"] ?? 0, width: region["width"] ?? 0, height: region["height"] ?? 0)
    #expect(asked == placement.look)
    #expect(look["source"] as? [String: AnyHashable] == ["kind": "display", "id": 1])
}

@MainActor
@Test func `a hot device asks for half of its pixels per point until it cools down`() async throws {
    let runtime = FeedRuntime()
    runtime.screen.viewScale = 3
    let watching = Task { await runtime.screen.watch() }
    defer { watching.cancel() }
    _ = try await runtime.open()

    runtime.store.hot = true
    let hot = try await runtime.next("size")["size"] as? [String: Double]
    #expect(hot?["scale"] == 1.5)
    runtime.store.hot = false
    let cool = try await runtime.next("size")["size"] as? [String: Double]
    #expect(cool?["scale"] == 3)
}

@MainActor
@Test func `a picture that a watch leaves behind is kept until the next watch shows a frame`() async throws {
    let runtime = FeedRuntime()
    runtime.screen.show(on: NSObject(), Shown())
    var watching = Task { await runtime.screen.watch() }
    defer { watching.cancel() }
    var pipe = try await runtime.open()
    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    await eventually { runtime.screen.picture == .live }

    try await runtime.leave(watching)
    #expect(runtime.screen.picture == .kept)

    watching = Task { await runtime.screen.watch() }
    pipe = try await runtime.open()
    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    await eventually { runtime.screen.picture == .live }
    #expect(runtime.screen.picture == .live)
}

@MainActor
@Test func `the screen says only the first reason that it does not show, and shows again as each reason clears`() async throws {
    let runtime = FeedRuntime()
    try runtime.store.observe([FeedRuntime.terminal(remote: true, shared: true)])
    runtime.screen.show(on: NSObject(), Shown())
    let watching = Task { await runtime.screen.watch() }
    defer { watching.cancel() }
    let pipe = try await runtime.open()
    #expect(runtime.screen.showing == .opening)
    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    await eventually { runtime.screen.showing == .live }

    try runtime.lose(pipe, saying: "The screen stopped.")
    await eventually { runtime.screen.showing == .failed }
    try runtime.say(screenState(recording: false), on: pipe)
    await eventually { runtime.screen.showing == .away(.unrecorded) }
    try runtime.say(screenState(recording: false, away: "locked"), on: pipe)
    await eventually { runtime.screen.showing == .away(.locked) }
    try runtime.store.observe([FeedRuntime.terminal(remote: true, offline: true, shared: true)])
    #expect(runtime.screen.showing == .away(.offline))
    try runtime.store.observe([FeedRuntime.terminal(remote: true, offline: true, shared: false)])
    #expect(runtime.screen.showing == .away(.sharingOff))

    try runtime.store.observe([FeedRuntime.terminal(remote: true, offline: true, shared: true)])
    #expect(runtime.screen.showing == .away(.offline))
    try runtime.store.observe([FeedRuntime.terminal(remote: true, shared: true)])
    #expect(runtime.screen.showing == .away(.locked))
}

@MainActor
@Test func `the screen's pipe closes while its computer does not share it, and opens once it does`() async throws {
    let runtime = FeedRuntime()
    try runtime.store.observe([FeedRuntime.terminal(remote: true, shared: true)])
    let watching = Task { await runtime.screen.watch() }
    _ = try await runtime.open()

    try runtime.store.observe([FeedRuntime.terminal(remote: true, shared: false)])
    try await runtime.closed()
    await watching.value
    await runtime.screen.watch()
    #expect(!runtime.opening())

    try runtime.store.observe([FeedRuntime.terminal(remote: true, shared: true)])
    let again = Task { await runtime.screen.watch() }
    defer { again.cancel() }
    _ = try await runtime.open()
}

@MainActor
@Test func `text copied on the computer goes to this device's clipboard once`() async throws {
    let runtime = FeedRuntime()
    let copied = Copied()
    runtime.screen.copied = { copied.words.append($0) }
    let watching = Task { await runtime.screen.watch() }
    defer { watching.cancel() }
    let pipe = try await runtime.open()

    try runtime.say(screenState(copied: "ls -la"), on: pipe)
    try runtime.say(screenState(width: 1440), on: pipe)
    await eventually { runtime.screen.size.width == 1440 }
    #expect(copied.words == ["ls -la"])
}

/// What the screen tells a viewer that controls it, which shows a display of `width` points.
private func screenState(width: Double = 1600, recording: Bool = true, away: String? = nil, copied: String? = nil) -> [String: Any] {
    var state: [String: Any] = [
        "source": ["kind": "display", "id": 1], "width": width, "height": 1000, "access": ["screen": recording, "input": true],
        "control": true,
    ]
    state["away"] = away
    state["copied"] = copied
    return state
}

@MainActor
private final class Copied {
    var words: [String] = []
}

@MainActor
private func eventually(_ done: () -> Bool) async {
    let deadline = ContinuousClock.now + .seconds(2)
    while !done(), ContinuousClock.now < deadline {
        try? await Task.sleep(for: .milliseconds(5))
    }
}
