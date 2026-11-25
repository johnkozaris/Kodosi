import Foundation
@testable import KodosiBridge
import KodosiKit
import Synchronization
import Testing

@MainActor
@Test func `a view that the host skipped goes on at an anchor, and one that lost a frame asks for a key frame`() async throws {
    let runtime = FeedRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject()) { _ in shown.add() }
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
    runtime.feed.show(on: NSObject()) { _ in shown.add() }
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
    runtime.feed.show(on: NSObject()) { _ in shown.add() }
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

private final class Shown: Sendable {
    private let shown = Mutex(0)

    var count: Int {
        shown.withLock { $0 }
    }

    func add() -> Bool {
        shown.withLock { $0 += 1 }
        return true
    }
}
