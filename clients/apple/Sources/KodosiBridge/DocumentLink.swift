import Foundation
import Synchronization

/// Manages a runtime document pipe, which the runtime reopens after each drop until the view
/// closes it. Shared-browser pipes receive events on the `BrowserPipes` task and may send frames
/// from both that task and the main actor, each frame in one piece.
public final class DocumentLink: Sendable {
    public enum News: Equatable, Sendable {
        /// Pipe opened (including after reconnect); callers should send their initial handshake.
        case open
        /// Pipe stayed down for a while; associated value is the runtime's error message, if any.
        case lost(String?)
        /// Terminal closed permanently; the pipe will not reconnect.
        case closed
    }

    private struct State {
        var pipe: String?
        var filling = Data()
        /// Next expected chunk sequence number, or `nil` before `document.opened`.
        var next: UInt64?
        /// Pipe ID awaiting its `document.open` response.
        var registering: String?
        var onFrame: @Sendable (Data) -> Void = { _ in }
        var onNews: @Sendable (News) -> Void = { _ in }
    }

    private enum Heard {
        case nothing
        case news(News, @Sendable (News) -> Void)
        case frame(Data, @Sendable (Data) -> Void)
        case broken
    }

    private static let partBytes = 256 * 1024

    private let commands: CommandSink
    let session: String
    private let key: String
    private let claims: BrowserPipes?
    private let state = Mutex(State())

    init(commands: CommandSink, session: String, key: String, claims: BrowserPipes? = nil) {
        self.commands = commands
        self.session = session
        self.key = key
        self.claims = claims
    }

    public var pipe: String? {
        state.withLock { $0.pipe }
    }

    public var onFrame: @Sendable (Data) -> Void {
        get { state.withLock { $0.onFrame } }
        set { state.withLock { $0.onFrame = newValue } }
    }

    public var onNews: @Sendable (News) -> Void {
        get { state.withLock { $0.onNews } }
        set { state.withLock { $0.onNews = newValue } }
    }

    @MainActor
    public func open() async throws {
        let opening = UUIDv7.generate()
        let earlier = state.withLock { state in
            defer {
                state.pipe = opening
                state.filling = Data()
                state.next = nil
            }
            return state.pipe
        }
        if let earlier {
            claims?.release(earlier)
        }
        claims?.claim(opening, for: self)
        do {
            try await register(opening)
        } catch {
            if forget(opening) {
                claims?.release(opening)
            }
            throw error
        }
    }

    public func send(_ frame: Data) {
        state.withLock { state in
            guard let pipe = state.pipe else { return }
            var offset = 0
            while offset < frame.count {
                let end = min(offset + Self.partBytes, frame.count)
                _ = commands.post("document.send", [
                    "pipe": .string(pipe), "more": .bool(end < frame.count),
                    "data": .string(frame[offset ..< end].base64EncodedString()),
                ])
                offset = end
            }
        }
    }

    public func close() {
        let pipe = state.withLock { state in
            defer { state.pipe = nil }
            return state.pipe
        }
        guard let pipe else { return }
        claims?.release(pipe)
        _ = commands.post("document.close", ["pipe": .string(pipe)])
    }

    /// Restarts the pipe after dropped events may have lost chunks. Pipes still awaiting
    /// registration retry on their own in `register(_:)`.
    func renewAfterLoss() {
        let pipe = state.withLock { $0.registering == $0.pipe ? nil : $0.pipe }
        if let pipe {
            renew(pipe)
        }
    }

    func receive(_ event: RuntimeEvent) {
        guard let pipe = event.string("pipe"), pipe == self.pipe else { return }
        switch event.type == "document.frame" ? part(event, of: pipe) : news(event, of: pipe) {
        case .nothing:
            break
        case let .news(news, onNews):
            if news == .closed {
                claims?.release(pipe)
            }
            onNews(news)
        case let .frame(frame, onFrame):
            onFrame(frame)
        case .broken:
            renew(pipe)
        }
    }

    private func news(_ event: RuntimeEvent, of pipe: String) -> Heard {
        state.withLock { state in
            guard state.pipe == pipe else { return .nothing }
            switch event.type {
            case "document.opened":
                state.filling = Data()
                state.next = 0
                return .news(.open, state.onNews)
            case "document.lost":
                state.filling = Data()
                return .news(.lost(event.string("message")), state.onNews)
            case "document.closed":
                state.pipe = nil
                return .news(.closed, state.onNews)
            default:
                return .nothing
            }
        }
    }

    private func part(_ event: RuntimeEvent, of pipe: String) -> Heard {
        let sequence = try? event.value("sequence", as: UInt64.self)
        let more = try? event.value("more", as: Bool.self)
        let data = event.string("data").flatMap { Data(base64Encoded: $0) }
        return state.withLock { state in
            guard state.pipe == pipe, let expected = state.next else { return .nothing }
            guard sequence == expected, let more, let data else { return .broken }
            state.next = expected + 1
            state.filling.append(data)
            guard !more else { return .nothing }
            defer { state.filling = Data() }
            return .frame(state.filling, state.onFrame)
        }
    }

    /// Retries registration after a timeout, closing first in case the runtime opened the pipe
    /// before the reply was dropped.
    @MainActor
    private func register(_ pipe: String) async throws {
        state.withLock { $0.registering = pipe }
        defer {
            state.withLock { state in
                if state.registering == pipe {
                    state.registering = nil
                }
            }
        }
        while true {
            do {
                _ = try await commands.request("document.open", [
                    "sessionId": .string(session), "pipe": .string(pipe), "key": .string(key),
                ])
                return
            } catch RuntimeError.timedOut where self.pipe == pipe {
                _ = commands.post("document.close", ["pipe": .string(pipe)])
            }
        }
    }

    /// Resets framing and re-registers the pipe after a missing chunk; clearing `next` drops any
    /// remaining in-flight chunks from the broken stream.
    private func renew(_ pipe: String) {
        let renews = state.withLock { state in
            guard state.pipe == pipe else { return false }
            state.filling = Data()
            state.next = nil
            state.registering = pipe
            return true
        }
        guard renews else { return }
        _ = commands.post("document.close", ["pipe": .string(pipe)])
        Task { @MainActor [weak self] in
            guard let self, self.pipe == pipe else { return }
            do {
                try await register(pipe)
            } catch {
                guard forget(pipe) else { return }
                claims?.release(pipe)
                onNews(.closed)
            }
        }
    }

    private func forget(_ pipe: String) -> Bool {
        state.withLock { state in
            guard state.pipe == pipe else { return false }
            state.pipe = nil
            return true
        }
    }
}
