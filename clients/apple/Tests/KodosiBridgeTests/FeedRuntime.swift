import CoreGraphics
import Foundation
@testable import KodosiBridge
import KodosiKit
import Synchronization
import Testing

/// A frame as the runtime hands it to a feed: its codec number, the part of the source that it
/// shows, and its Annex B picture.
struct TestFrame {
    let codec: UInt8
    let region: CGRect?
    let picture: Data
}

/// The runtime's side of the app's watches, which counts the keyframes that the feeds ask for.
final class Watches: VideoWatching {
    private let state = Mutex<(watched: Set<String>, restarts: Int)>(([], 0))

    var restarts: Int {
        state.withLock { $0.restarts }
    }

    func watches(_ pipe: String) -> Bool {
        state.withLock { $0.watched.contains(pipe) }
    }

    func watch(_ pipe: String) {
        state.withLock { _ = $0.watched.insert(pipe) }
    }

    func restart(_: String) {
        state.withLock { $0.restarts += 1 }
    }

    func unwatch(_ pipe: String) {
        state.withLock { _ = $0.watched.remove(pipe) }
    }
}

/// A runtime that a feed's pipe talks to: it opens the pipe, hands it frames and messages, and
/// keeps what the feed sends.
@MainActor
final class FeedRuntime {
    static let keyFrame: TestFrame = {
        let start: [UInt8] = [0, 0, 0, 1]
        let sequenceParameters: [UInt8] = [0x27, 0x42, 0x00, 0x0B, 0xAB, 0x41, 0x86, 0xF0, 0x2C, 0xC1, 0x4A]
        let pictureParameters: [UInt8] = [0x28, 0xCE, 0x3C, 0x80]
        let keyPicture: [UInt8] = [0x25, 0x88, 0x84]
        return TestFrame(codec: 0, region: nil,
                         picture: Data(start + sequenceParameters + start + pictureParameters + start + keyPicture))
    }()

    /// A delta frame, which shows only `region` of the source when it has one.
    static func frame(region: CGRect? = nil) -> TestFrame {
        TestFrame(codec: 0, region: region, picture: Data([0, 0, 0, 1, 0x21, 0xE0, 0x84]))
    }

    /// The parameter sets and picture of a 64x64 HEVC keyframe from VideoToolbox.
    static let hevcKeyFrame: TestFrame = {
        let start: [UInt8] = [0, 0, 0, 1]
        let videoParameters: [UInt8] = [
            0x40, 0x01, 0x0C, 0x03, 0xFF, 0xFF, 0x01, 0x60, 0x00, 0x00, 0x03, 0x00, 0xB0, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03,
            0x00, 0x3C, 0x00, 0x00, 0x04, 0x30, 0x24,
        ]
        let sequenceParameters: [UInt8] = [
            0x42, 0x01, 0x03, 0x01, 0x60, 0x00, 0x00, 0x03, 0x00, 0xB0, 0x00, 0x00, 0x03, 0x00, 0x00, 0x03, 0x00, 0x3C, 0x00,
            0x00, 0xA0, 0x14, 0x20, 0x41, 0xC1, 0x8F, 0x88, 0x04, 0x3B, 0x91, 0x48, 0x20, 0xB9, 0xF8, 0x4F, 0x42, 0xFA, 0x86,
            0xF5, 0x43, 0xFA, 0xA8, 0x23, 0xD5, 0x50, 0x4F, 0xAA, 0xA8, 0x2B, 0xD5, 0x55, 0x05, 0xFA, 0xAA, 0xA8, 0x33, 0xD5,
            0x55, 0x50, 0x6F, 0xAA, 0xAA, 0xA8, 0x3B, 0xD5, 0x55, 0x55, 0x07, 0xFA, 0xAA, 0xAA, 0xA8, 0x10, 0xF5, 0x55, 0x55,
            0x54, 0xA6, 0xA0, 0x20, 0x20, 0x20, 0x7F, 0x08, 0x04, 0x10,
        ]
        let pictureParameters: [UInt8] = [0x44, 0x01, 0xC0, 0x72, 0xF0, 0x5B, 0x24]
        let keyPicture: [UInt8] = [0x28, 0x01, 0xAF, 0x0A, 0xDA, 0x34, 0x70, 0x41]
        return TestFrame(codec: 1, region: nil, picture: Data(
            start + videoParameters + start + sequenceParameters + start + pictureParameters + start + keyPicture,
        ))
    }()

    static let hevcFrame = TestFrame(codec: 1, region: nil, picture: Data([0, 0, 0, 1, 0x02, 0x01, 0xD0, 0x01, 0x83, 0x10]))

    static let terminalId = "01992e43-53db-7040-8e02-f6ebf4149b28"

    /// A terminal of this computer, or of another one that may be offline, whose canvas shows the
    /// browser in `browser` state, and the computer's screen when `shared` says whether it is shared.
    static func terminal(remote: Bool = false, offline: Bool = false, browser: String = "awake", shared: Bool? = nil) throws -> RuntimeSession {
        var canvas: [String: Any] = [
            "sequence": 2,
            "items": [["key": "/tmp/work/plan.md", "name": "plan.md", "kind": "page", "revision": 1, "bytes": 1, "changed": 1]],
            "browser": ["state": browser, "changed": 2, "width": 1280, "height": 720],
        ]
        if let shared {
            canvas["screen"] = ["shared": shared]
        }
        let fields: [String: Any] = [
            "id": terminalId, "incarnationId": "01992e43-53db-7040-8e02-f6ebf4149b29", "kind": remote ? "remote" : "local",
            "name": "checkout", "hostName": "Mac mini", "isOwner": true, "status": offline ? "offline" : "running", "attention": false,
            "hidden": false, "working": false, "connectionState": remote ? "connected" : "local", "sharedWith": [Any](),
            "canvas": canvas,
        ]
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONSerialization.data(withJSONObject: fields))
    }

    let feed: BrowserFeed
    let screen: ScreenFeed
    let store: CanvasStore
    let watches = Watches()
    private(set) var opened: Bool?
    private var pipe: String?
    private let commands = CommandSink()
    private let pipes = VideoPipes()
    private let log = ProjectCommandLog()

    init() {
        let log = log
        commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        commands.setAccount(userId: "user-1", epoch: 1)
        pipes.attach(watches)
        store = CanvasStore(commands: commands, videoPipes: pipes)
        feed = store.feed(Self.terminalId)
        screen = store.screenFeed(Self.terminalId)
    }

    /// Only expects `document.open` on the first call; reconnects reuse the same pipe.
    func open() async throws -> String {
        if pipe == nil {
            let opening = try await log.next("document.open")
            try commands.receive(Self.event("document.result", ["requestId": opening["requestId"] ?? "", "operation": "document.open"]))
            pipe = try #require(opening["pipe"] as? String)
        }
        let pipe = try #require(pipe)
        let deadline = ContinuousClock.now + .seconds(2)
        while !watches.watches(pipe), ContinuousClock.now < deadline {
            await Task.yield()
        }
        #expect(watches.watches(pipe), "a video pipe is watched once it opened")
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

    /// Hands `frame` to the feed as the runtime's thread of the pipe does, and returns whether its
    /// decoder took it.
    @discardableResult
    func send(_ frame: TestFrame, on pipe: String) -> Bool {
        frame.picture.withUnsafeBytes { bytes in
            VideoFrame(codec: frame.codec, region: frame.region, bytes: bytes).map { pipes.frame(pipe, $0) }
                ?? false
        }
    }

    func say(_ message: [String: Any], on pipe: String) throws {
        try pipes.message(pipe, JSONSerialization.data(withJSONObject: message))
    }

    func stale(_ pipe: String, _ stale: Bool) {
        pipes.stale(pipe, stale)
    }

    /// The fields of the next message that the feed sends of `type`.
    func next(_ type: String) async throws -> [String: Any] {
        while true {
            if let sent = try await Self.body(log.next("document.send")), sent["type"] as? String == type {
                return sent
            }
        }
    }

    func lose(_ pipe: String, saying message: String? = nil) throws {
        var fields: [String: Any] = ["pipe": pipe]
        if let message {
            fields["message"] = message
        }
        try deliver("document.lost", fields)
    }

    /// Waits for the feed to close its pipe.
    func closed() async throws {
        _ = try await log.next("document.close")
        pipe = nil
    }

    /// Whether the feed asked to open a pipe since the last time.
    func opening() -> Bool {
        log.take("document.open") != nil
    }

    /// Answers the next request that the canvas makes, and returns what it asked.
    func answer() async throws -> [String: Any] {
        let request = try await log.next("canvas.request")
        try commands.receive(Self.event("canvas.result", [
            "requestId": request["requestId"] ?? "", "operation": "canvas.request", "answer": ["answer": "done"],
        ]))
        return request["request"] as? [String: Any] ?? [:]
    }

    /// The requests that the canvas made and that nothing answered yet.
    func unanswered() -> Int {
        var count = 0
        while log.take("canvas.request") != nil {
            count += 1
        }
        return count
    }

    func messages() -> [String?] {
        var messages: [String?] = []
        while let command = log.take("document.send") {
            messages.append(Self.message(command))
        }
        return messages
    }

    private func deliver(_ type: String, _ fields: [String: Any]) throws {
        try store.receive(Self.event(type, fields))
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

/// Keeps the regions of the frames that a feed shows on it, and counts the watches that stopped.
final class Shown: VideoDisplay {
    private let shown = Mutex<[CGRect?]>([])
    private let stopped = Mutex(0)

    var regions: [CGRect?] {
        shown.withLock { $0 }
    }

    var count: Int {
        regions.count
    }

    var stops: Int {
        stopped.withLock { $0 }
    }

    func enqueue(_ sample: VideoSample) -> Bool {
        shown.withLock { $0.append(sample.region) }
        return true
    }

    func stop() {
        stopped.withLock { $0 += 1 }
    }
}
