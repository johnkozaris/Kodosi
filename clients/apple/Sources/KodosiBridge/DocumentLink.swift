import Foundation

/// The runtime keeps the pipe of a view and opens it again after each loss, until the view
/// closes it.
@MainActor
public final class DocumentLink {
    public enum News: Equatable, Sendable {
        /// The pipe is open, again after each loss: the view sends its first message.
        case open
        /// The pipe stayed lost for a while, for the reason that the runtime gives.
        case lost(String?)
        /// The terminal of the pipe is gone.
        case closed
    }

    private static let partBytes = 256 * 1024

    private let commands: CommandSink
    let session: String
    private let key: String
    private var filling = Data()
    /// The next part, or none while a new pipe has not opened.
    private var next: UInt64?
    /// The pipe that waits for the answer to its registration.
    private var registering: String?
    public private(set) var pipe: String?
    public var onFrame: (Data) -> Void = { _ in }
    public var onNews: (News) -> Void = { _ in }

    init(commands: CommandSink, session: String, key: String) {
        self.commands = commands
        self.session = session
        self.key = key
    }

    public func open() async throws {
        let opening = UUIDv7.generate()
        pipe = opening
        filling = Data()
        next = nil
        do {
            try await register(opening)
        } catch {
            if pipe == opening {
                pipe = nil
            }
            throw error
        }
    }

    public func send(_ frame: Data) {
        guard let pipe else { return }
        var offset = 0
        while offset < frame.count {
            let end = min(offset + Self.partBytes, frame.count)
            commands.send("document.send", [
                "pipe": .string(pipe), "more": .bool(end < frame.count),
                "data": .string(frame[offset ..< end].base64EncodedString()),
            ])
            offset = end
        }
    }

    public func close() {
        guard let pipe else { return }
        self.pipe = nil
        commands.send("document.close", ["pipe": .string(pipe)])
    }

    /// The app dropped events when it fell behind, maybe parts of this pipe, so the pipe starts
    /// again. A pipe that waits for its registration registers again by itself.
    func renewAfterLoss() {
        guard let pipe, registering != pipe else { return }
        renew(pipe)
    }

    func receive(_ event: RuntimeEvent) {
        guard let pipe, event.string("pipe") == pipe else { return }
        switch event.type {
        case "document.opened":
            filling = Data()
            next = 0
            onNews(.open)
        case "document.lost":
            filling = Data()
            onNews(.lost(event.string("message")))
        case "document.closed":
            self.pipe = nil
            onNews(.closed)
        case "document.frame":
            guard let expected = next else { return }
            guard (try? event.value("sequence", as: UInt64.self)) == expected, let more = try? event.value("more", as: Bool.self),
                  let data = event.string("data").flatMap({ Data(base64Encoded: $0) })
            else {
                renew(pipe)
                return
            }
            next = expected + 1
            filling.append(data)
            if !more {
                onFrame(filling)
                filling = Data()
            }
        default:
            break
        }
    }

    /// An answer that went with the events that the app dropped leaves the pipe unknown, so the
    /// link closes it and registers it again.
    private func register(_ pipe: String) async throws {
        registering = pipe
        defer {
            if registering == pipe {
                registering = nil
            }
        }
        while true {
            do {
                _ = try await commands.request("document.open", [
                    "sessionId": .string(session), "pipe": .string(pipe), "key": .string(key),
                ])
                return
            } catch RuntimeError.timedOut where self.pipe == pipe {
                commands.send("document.close", ["pipe": .string(pipe)])
            }
        }
    }

    /// A part that went missing breaks the frames, so the pipe starts again, and the parts that
    /// were on their way go nowhere.
    private func renew(_ pipe: String) {
        filling = Data()
        next = nil
        commands.send("document.close", ["pipe": .string(pipe)])
        registering = pipe
        Task { [weak self] in
            guard let self, self.pipe == pipe else { return }
            do {
                try await register(pipe)
            } catch {
                guard self.pipe == pipe else { return }
                self.pipe = nil
                onNews(.closed)
            }
        }
    }
}
