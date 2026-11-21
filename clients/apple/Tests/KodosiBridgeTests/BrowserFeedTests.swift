import Foundation
@testable import KodosiBridge
import KodosiKit
import Synchronization
import Testing

@MainActor
@Test func `each copy that the browser sends reaches the pasteboard, also the same words again`() async throws {
    let runtime = BrowserRuntime()
    var copies: [String] = []
    runtime.feed.copied = { copies.append($0) }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }
    let copy = Data([0]) + Data(#"{"width":1280,"height":720,"shape":"","pointers":[],"copied":"word"}"#.utf8)
    let later = Data([0]) + Data(#"{"width":1280,"height":720,"shape":"text","pointers":[]}"#.utf8)

    let pipe = try await runtime.open()
    try runtime.send(copy, on: pipe)
    try runtime.send(copy, on: pipe)
    try runtime.send(later, on: pipe)
    try await until { runtime.feed.window.shape == "text" }

    #expect(copies == ["word", "word"])
}

@MainActor
@Test func `the page keeps its last picture while the browser joins again, until a new frame replaces it`() async throws {
    let runtime = BrowserRuntime()
    runtime.feed.show(on: NSObject()) { _ in true }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    let first = try await runtime.open()
    try runtime.send(BrowserRuntime.keyFrame, on: first)
    try await until { runtime.feed.picture == .live }
    try runtime.lose(first)
    let second = try await runtime.open()
    #expect(runtime.feed.picture == .kept)
    try runtime.send(BrowserRuntime.keyFrame, on: second)
    try await until { runtime.feed.picture == .live }
}

@MainActor
@Test func `a new surface has no picture until the key frame that it asks the host for`() async throws {
    let runtime = BrowserRuntime()
    let surface = NSObject()
    runtime.feed.show(on: surface) { _ in true }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    try await runtime.send(BrowserRuntime.keyFrame, on: runtime.open())
    try await until { runtime.feed.picture == .live }
    runtime.feed.show(on: surface) { _ in true }
    #expect(runtime.feed.picture == .live)
    runtime.feed.show(on: NSObject()) { _ in true }
    #expect(runtime.feed.picture == .waiting)
    #expect(try await runtime.messages(2) == ["ack", "keyframe"])
}

@MainActor
@Test func `a view that the host skipped goes on at an anchor, and one that lost a frame asks for a key frame`() async throws {
    let runtime = BrowserRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject()) { _ in shown.add() }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    let pipe = try await runtime.open()
    try runtime.send(BrowserRuntime.keyFrame, on: pipe)
    try runtime.send(BrowserRuntime.frame(4, anchor: true), on: pipe)
    try runtime.send(BrowserRuntime.frame(5), on: pipe)
    try runtime.send(BrowserRuntime.frame(7), on: pipe)
    try runtime.send(BrowserRuntime.frame(9, anchor: true), on: pipe)
    #expect(try await runtime.messages(6) == ["ack", "ack", "ack", "ack", "keyframe", "ack"])
    #expect(shown.count == 3)
}

@MainActor
@Test func `a frame is acknowledged and shown while the main actor is busy`() async throws {
    let runtime = BrowserRuntime()
    let shown = Shown()
    runtime.feed.show(on: NSObject()) { _ in shown.add() }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }
    let pipe = try await runtime.open()

    try runtime.send(BrowserRuntime.keyFrame, on: pipe)
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
@Test func `a view says that a person opened the browser, but not when it joins again after a lost link`() async throws {
    let runtime = BrowserRuntime()
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    let first = try await runtime.open()
    #expect(runtime.opened == true)
    try runtime.lose(first)
    _ = try await runtime.open()
    #expect(runtime.opened == false)
}

@Test func `taps in about one place count up as clicks, so two select a word, and a later or farther tap is one click again`() {
    var taps = BrowserTaps()
    let word = CGPoint(x: 100, y: 200)

    #expect(taps.click(at: word, time: 10) == 1)
    #expect(taps.click(at: CGPoint(x: 112, y: 206), time: 10.3) == 2)
    #expect(taps.click(at: word, time: 10.6) == 3)
    #expect(taps.click(at: word, time: 12) == 1)
    #expect(taps.click(at: CGPoint(x: 300, y: 200), time: 12.2) == 1)
    taps.forget()
    #expect(taps.click(at: CGPoint(x: 300, y: 200), time: 12.3) == 1)
}

/// Waits for what the feed's view reads, which comes on the main actor after the pipe's task.
@MainActor
private func until(_ holds: () -> Bool) async throws {
    let deadline = ContinuousClock.now + .seconds(2)
    while !holds() {
        guard ContinuousClock.now < deadline else { throw ProjectTestFailure.missing("a change of the feed") }
        try await Task.sleep(for: .milliseconds(2))
    }
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

@MainActor
private final class BrowserRuntime {
    static let keyFrame: Data = {
        let start: [UInt8] = [0, 0, 0, 1]
        let header: [UInt8] = [1, 0, 0, 0, 1, 1]
        let sequenceParameters: [UInt8] = [0x27, 0x42, 0x00, 0x0B, 0xAB, 0x41, 0x86, 0xF0, 0x2C, 0xC1, 0x4A]
        let pictureParameters: [UInt8] = [0x28, 0xCE, 0x3C, 0x80]
        let keyPicture: [UInt8] = [0x25, 0x88, 0x84]
        return Data(header + start + sequenceParameters + start + pictureParameters + start + keyPicture)
    }()

    static func frame(_ id: UInt32, anchor: Bool = false) -> Data {
        let start: [UInt8] = [0, 0, 0, 1]
        let number = withUnsafeBytes(of: id.bigEndian, Array.init)
        let header: [UInt8] = [1] + number + [anchor ? 2 : 0]
        let picture: [UInt8] = [0x21, 0xE0, 0x84]
        return Data(header + start + picture)
    }

    let feed: BrowserFeed
    private(set) var opened: Bool?
    private var pipe: String?
    private let commands = CommandSink()
    private let pipes = BrowserPipes()
    private let store: CanvasStore
    private let log = ProjectCommandLog()
    private var sequences: [String: UInt64] = [:]

    init() {
        let log = log
        commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        commands.setAccount(userId: "user-1", epoch: 1)
        store = CanvasStore(commands: commands, browserPipes: pipes)
        feed = store.feed("terminal")
    }

    /// The view opens the pipe once, and the runtime opens it again after each loss.
    func open() async throws -> String {
        if pipe == nil {
            let opening = try await log.next("document.open")
            try commands.receive(Self.event("document.result", ["requestId": opening["requestId"] ?? "", "operation": "document.open"]))
            pipe = try #require(opening["pipe"] as? String)
        }
        let pipe = try #require(pipe)
        sequences[pipe] = 0
        try deliver("document.opened", ["pipe": pipe])
        var sent = try await Self.body(log.next("document.send"))
        while sent?["type"] as? String != "watch" {
            sent = try await Self.body(log.next("document.send"))
        }
        opened = sent?["opened"] as? Bool
        return pipe
    }

    func send(_ frame: Data, on pipe: String) throws {
        let sequence = sequences[pipe, default: 0]
        sequences[pipe] = sequence + 1
        try deliver("document.frame", ["pipe": pipe, "sequence": sequence, "more": false, "data": frame.base64EncodedString()])
    }

    func lose(_ pipe: String) throws {
        try deliver("document.lost", ["pipe": pipe])
    }

    func messages() -> [String?] {
        var messages: [String?] = []
        while let command = log.take("document.send") {
            messages.append(Self.message(command))
        }
        return messages
    }

    /// The pipe's task sends its acknowledgments on its own time.
    func messages(_ count: Int) async throws -> [String?] {
        var messages: [String?] = []
        while messages.count < count {
            try await messages.append(Self.message(log.next("document.send")))
        }
        return messages
    }

    /// The events of a pipe go the way that the runtime's callback gives them.
    private func deliver(_ type: String, _ fields: [String: Any]) throws {
        #expect(try pipes.take(Self.data(type, fields)))
    }

    private static func data(_ type: String, _ fields: [String: Any]) throws -> Data {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        return try JSONSerialization.data(withJSONObject: payload)
    }

    private static func event(_ type: String, _ fields: [String: Any]) throws -> RuntimeEvent {
        try JSONDecoder().decode(RuntimeEvent.self, from: data(type, fields))
    }

    private static func body(_ command: [String: Any]) -> [String: Any]? {
        guard let data = (command["data"] as? String).flatMap({ Data(base64Encoded: $0) }) else { return nil }
        return try? JSONSerialization.jsonObject(with: data.dropFirst()) as? [String: Any]
    }

    private static func message(_ command: [String: Any]) -> String? {
        body(command)?["type"] as? String
    }
}
