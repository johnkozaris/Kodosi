import Foundation
import Synchronization

/// The runtime keeps the pipe of a view and opens it again after each loss, until the view
/// closes it. The pipe of the shared browser takes its events on the task of the browser pipes,
/// and sends from there and from the main actor, each frame in one piece.
public final class DocumentLink: Sendable {
    public enum News: Equatable, Sendable {
        /// The pipe is open, again after each loss: the view sends its first message.
        case open
        /// The pipe stayed lost for a while, for the reason that the runtime gives.
        case lost(String?)
        /// The terminal of the pipe is gone.
        case closed
    }

    private struct State {
        var pipe: String?
        var filling = Data()
        /// The next part, or none while a new pipe has not opened.
        var next: UInt64?
        /// The pipe that waits for the answer to its registration.
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
    /// The pipes of the shared browser, which claim the pipe of the browser's link alone.
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

    /// The app dropped events when it fell behind, maybe parts of this pipe, so the pipe starts
    /// again. A pipe that waits for its registration registers again by itself.
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

    /// An answer that went with the events that the app dropped leaves the pipe unknown, so the
    /// link closes it and registers it again.
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

    /// A part that went missing breaks the frames, so the pipe starts again, and the parts that
    /// were on their way go nowhere.
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

    /// Returns whether the pipe was still the link's.
    private func forget(_ pipe: String) -> Bool {
        state.withLock { state in
            guard state.pipe == pipe else { return false }
            state.pipe = nil
            return true
        }
    }
}
