import Foundation
@testable import KodosiBridge
import KodosiKit
import Synchronization
import Testing

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
@Test func `the page takes the shape of a view only after a person opens the browser there`() async throws {
    let runtime = BrowserRuntime()
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
    let runtime = BrowserRuntime()
    let session = try BrowserRuntime.terminal()
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

    static let terminalId = "01992e43-53db-7040-8e02-f6ebf4149b28"

    static func terminal() throws -> RuntimeSession {
        let fields: [String: Any] = [
            "id": terminalId, "incarnationId": "01992e43-53db-7040-8e02-f6ebf4149b29", "kind": "local", "name": "checkout",
            "isOwner": true, "status": "running", "attention": false, "hidden": false, "working": false,
            "connectionState": "local", "sharedWith": [Any](),
            "canvas": [
                "sequence": 2,
                "items": [["key": "/tmp/work/plan.md", "name": "plan.md", "kind": "page", "revision": 1, "bytes": 1, "changed": 1]],
                "browser": ["state": "awake", "changed": 2, "width": 1280, "height": 720],
            ],
        ]
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONSerialization.data(withJSONObject: fields))
    }

    let feed: BrowserFeed
    let store: CanvasStore
    private(set) var opened: Bool?
    private var pipe: String?
    private let commands = CommandSink()
    private let pipes = BrowserPipes()
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
        feed = store.feed(Self.terminalId)
    }

    /// Only expects `document.open` on the first call; reconnects reuse the same pipe.
    func open() async throws -> String {
        if pipe == nil {
            let opening = try await log.next("document.open")
            try commands.receive(Self.event("document.result", ["requestId": opening["requestId"] ?? "", "operation": "document.open"]))
            pipe = try #require(opening["pipe"] as? String)
        }
        let pipe = try #require(pipe)
        sequences[pipe] = 0
        try deliver("document.opened", ["pipe": pipe])
        opened = try await watched()
        return pipe
    }

    func watched() async throws -> Bool? {
        var sent = try await Self.body(log.next("document.send"))
        while sent?["type"] as? String != "watch" {
            sent = try await Self.body(log.next("document.send"))
        }
        return sent?["opened"] as? Bool
    }

    func leave(_ watching: Task<Void, Never>) async throws {
        watching.cancel()
        await watching.value
        _ = try await log.next("document.close")
        pipe = nil
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

    /// Awaits `count` outbound messages emitted asynchronously by the pipe task.
    func messages(_ count: Int) async throws -> [String?] {
        var messages: [String?] = []
        while messages.count < count {
            try await messages.append(Self.message(log.next("document.send")))
        }
        return messages
    }

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
