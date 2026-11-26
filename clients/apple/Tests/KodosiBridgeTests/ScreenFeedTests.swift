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
private func eventually(_ done: () -> Bool) async {
    let deadline = ContinuousClock.now + .seconds(2)
    while !done(), ContinuousClock.now < deadline {
        try? await Task.sleep(for: .milliseconds(5))
    }
}
