import Foundation
@testable import KodosiBridge
import KodosiKit
import Testing

@MainActor
@Test func `a view that the host skipped goes on at an anchor, and one that lost a frame asks for a key frame`() async throws {
    let runtime = FeedRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject(), shown)
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    let pipe = try await runtime.open()
    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    try runtime.send(FeedRuntime.frame(4, anchor: true), on: pipe)
    try runtime.send(FeedRuntime.frame(5), on: pipe)
    try runtime.send(FeedRuntime.frame(7), on: pipe)
    try runtime.send(FeedRuntime.frame(9, anchor: true), on: pipe)
    #expect(try await runtime.messages(6) == ["ack", "ack", "ack", "ack", "keyframe", "ack"])
    #expect(shown.count == 3)
}

@MainActor
@Test func `a stream that moves to HEVC at a key frame goes on showing`() async throws {
    let runtime = FeedRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject(), shown)
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    let pipe = try await runtime.open()
    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    try runtime.send(FeedRuntime.hevcKeyFrame(2), on: pipe)
    try runtime.send(FeedRuntime.hevcFrame(3), on: pipe)
    #expect(try await runtime.messages(3) == ["ack", "ack", "ack"])
    #expect(shown.count == 3)
}

@MainActor
@Test func `a frame is acknowledged and shown while the main actor is busy`() async throws {
    let runtime = FeedRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject(), shown)
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }
    let pipe = try await runtime.open()

    try runtime.send(FeedRuntime.keyFrame, on: pipe)
    var sent: [String?] = []
    let deadline = ContinuousClock.now + .seconds(2)
    while sent.isEmpty || shown.count == 0, ContinuousClock.now < deadline {
        sent += runtime.messages()
        usleep(500)
    }

    #expect(sent == ["ack"])
    #expect(shown.count == 1)
}

@MainActor
@Test func `the page takes the shape of a view only after a person opens the browser there`() async throws {
    let runtime = FeedRuntime()
    var watching = Task { await runtime.feed.watch() }
    let first = try await runtime.open()
    #expect(runtime.opened == false)
    runtime.feed.open()
    #expect(try await runtime.watched() == true)
    try runtime.lose(first)
    _ = try await runtime.open()
    #expect(runtime.opened == false)

    try await runtime.leave(watching)
    runtime.feed.open()
    watching = Task { await runtime.feed.watch() }
    _ = try await runtime.open()
    #expect(runtime.opened == true)

    try await runtime.leave(watching)
    watching = Task { await runtime.feed.watch() }
    _ = try await runtime.open()
    #expect(runtime.opened == false)
    try await runtime.leave(watching)
}

@MainActor
@Test func `an agent's show leaves the page where it is, and a person's open takes it to this device`() async throws {
    let runtime = FeedRuntime()
    let session = try FeedRuntime.terminal()
    let browser = try #require(session.canvas?.shown(CanvasShown.browserKey))
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }
    _ = try await runtime.open()

    runtime.store.reveal(browser, in: session)
    runtime.store.show(.files, in: session)
    #expect(!runtime.messages().contains("watch"))
    runtime.store.show(.browser, in: session)
    #expect(try await runtime.watched() == true)
}

@MainActor
@Test func `a browser that someone looks at wakes once each time that it falls asleep`() async throws {
    let runtime = FeedRuntime()
    try runtime.store.observe([FeedRuntime.terminal(browser: "asleep")])
    let first = Task { await runtime.feed.wakeIfAsleep() }
    #expect(try await runtime.answer()["action"] as? [String: String] == ["do": "wake"])
    await first.value
    await expectNoWake(runtime)

    try runtime.store.observe([FeedRuntime.terminal(browser: "awake")])
    await expectNoWake(runtime)
    try runtime.store.observe([FeedRuntime.terminal(browser: "asleep")])
    let again = Task { await runtime.feed.wakeIfAsleep() }
    #expect(try await runtime.answer()["action"] as? [String: String] == ["do": "wake"])
    await again.value
}

@MainActor
private func expectNoWake(_ runtime: FeedRuntime) async {
    let waking = Task { await runtime.feed.wakeIfAsleep() }
    await Task.yield()
    #expect(runtime.unanswered() == 0)
    waking.cancel()
}

@MainActor
@Test func `a watch that ends stops the display, so that it lets go of its decoder`() async throws {
    let runtime = FeedRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject(), shown)
    let watching = Task { await runtime.feed.watch() }
    _ = try await runtime.open()
    #expect(shown.stops == 0)

    try await runtime.leave(watching)
    #expect(shown.stops == 1)
}

@MainActor
@Test func `a view that goes away lets go of its player, but not of the player of a view that took its place`() {
    let runtime = FeedRuntime()
    let gone = NSObject()
    let next = NSObject()
    weak var player: Shown?
    runtime.feed.show(on: gone, Shown())
    do {
        let shown = Shown()
        player = shown
        runtime.feed.show(on: next, shown)
    }

    runtime.feed.hide(from: gone)
    #expect(player != nil)
    runtime.feed.hide(from: next)
    #expect(player == nil)
}
