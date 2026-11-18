import Foundation
@testable import KodosiBridge
import KodosiKit
import Testing

@MainActor
@Test func `each copy that the browser sends reaches the pasteboard, also the same words again`() {
    let feed = CanvasStore(commands: CommandSink()).feed("terminal")
    var copies: [String] = []
    feed.copied = { copies.append($0) }
    let copy = Data([0]) + Data(#"{"width":1280,"height":720,"shape":"","pointers":[],"copied":"word"}"#.utf8)
    let later = Data([0]) + Data(#"{"width":1280,"height":720,"shape":"text","pointers":[]}"#.utf8)

    feed.receive(copy)
    feed.receive(copy)
    feed.receive(later)

    #expect(copies == ["word", "word"])
}

@MainActor
@Test func `the page keeps its last picture while the browser joins again, until a new frame replaces it`() async throws {
    let runtime = BrowserRuntime()
    runtime.feed.show(on: NSObject()) { _ in }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    let first = try await runtime.open()
    try runtime.send(BrowserRuntime.keyFrame, on: first)
    #expect(runtime.feed.picture == .live)
    try runtime.lose(first)
    let second = try await runtime.open()
    #expect(runtime.feed.picture == .kept)
    try runtime.send(BrowserRuntime.keyFrame, on: second)
    #expect(runtime.feed.picture == .live)
}

@MainActor
@Test func `a new surface has no picture until the key frame that it asks the host for`() async throws {
    let runtime = BrowserRuntime()
    let surface = NSObject()
    runtime.feed.show(on: surface) { _ in }
    let watching = Task { await runtime.feed.watch() }
    defer { watching.cancel() }

    try await runtime.send(BrowserRuntime.keyFrame, on: runtime.open())
    runtime.feed.show(on: surface) { _ in }
    #expect(runtime.feed.picture == .live)
    runtime.feed.show(on: NSObject()) { _ in }
    #expect(runtime.feed.picture == .waiting)
    #expect(runtime.messages() == ["ack", "keyframe"])
}

@MainActor
private final class BrowserRuntime {
    static let keyFrame: Data = {
        let start: [UInt8] = [0, 0, 0, 1]
        let header: [UInt8] = [1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0]
        let sequenceParameters: [UInt8] = [0x27, 0x42, 0x00, 0x0B, 0xAB, 0x41, 0x86, 0xF0, 0x2C, 0xC1, 0x4A]
        let pictureParameters: [UInt8] = [0x28, 0xCE, 0x3C, 0x80]
        let keyPicture: [UInt8] = [0x25, 0x88, 0x84]
        return Data(header + start + sequenceParameters + start + pictureParameters + start + keyPicture)
    }()

    let feed: BrowserFeed
    private let commands = CommandSink()
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
        store = CanvasStore(commands: commands)
        feed = store.feed("terminal")
    }

    func open() async throws -> String {
        let opening = try await log.next("document.open")
        try deliver("document.result", ["requestId": opening["requestId"] ?? "", "operation": "document.open"])
        while try await Self.message(log.next("document.send")) != "watch" {}
        return try #require(opening["pipe"] as? String)
    }

    func send(_ frame: Data, on pipe: String) throws {
        let sequence = sequences[pipe, default: 0]
        sequences[pipe] = sequence + 1
        try deliver("document.frame", ["pipe": pipe, "sequence": sequence, "more": false, "data": frame.base64EncodedString()])
    }

    func lose(_ pipe: String) throws {
        try deliver("document.closed", ["pipe": pipe])
    }

    func messages() -> [String?] {
        var messages: [String?] = []
        while let command = log.take("document.send") {
            messages.append(Self.message(command))
        }
        return messages
    }

    private func deliver(_ type: String, _ fields: [String: Any]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let event = try JSONDecoder().decode(RuntimeEvent.self, from: JSONSerialization.data(withJSONObject: payload))
        commands.receive(event)
        store.receive(event)
    }

    private static func message(_ command: [String: Any]) -> String? {
        guard let data = (command["data"] as? String).flatMap({ Data(base64Encoded: $0) }),
              let message = try? JSONSerialization.jsonObject(with: data.dropFirst()) as? [String: Any]
        else { return nil }
        return message["type"] as? String
    }
}
