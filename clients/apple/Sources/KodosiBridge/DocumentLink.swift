import Foundation

/// One live document of a canvas on this device. Frames from the view go to the runtime in parts,
/// and the runtime's parts come back whole. When the pipe breaks, the link says so, and the view
/// joins again on a new pipe, at once when the terminal can be reached again.
@MainActor
public final class DocumentLink {
    private static let partBytes = 256 * 1024

    private let commands: CommandSink
    let session: String
    private let key: String
    private var filling = Data()
    private var next: UInt64 = 0
    private var pausing: CheckedContinuation<Bool, Never>?
    private var reachedWhileBusy = false
    public private(set) var pipe: String?
    public var onFrame: (Data) -> Void = { _ in }
    public var onLost: () -> Void = {}

    init(commands: CommandSink, session: String, key: String) {
        self.commands = commands
        self.session = session
        self.key = key
    }

    public func open() async throws {
        let opening = UUIDv7.generate()
        filling = Data()
        next = 0
        pipe = opening
        do {
            _ = try await commands.request("document.open", [
                "sessionId": .string(session), "pipe": .string(opening), "key": .string(key),
            ])
            reachedWhileBusy = false
        } catch {
            if pipe == opening {
                pipe = nil
            }
            throw error
        }
    }

    /// Waits `wait`, or less when the terminal of this link can be reached again. True when it can.
    public func pause(_ wait: Duration) async -> Bool {
        if reachedWhileBusy {
            reachedWhileBusy = false
            return true
        }
        let timer = Task { [weak self] in
            try? await Task.sleep(for: wait)
            self?.resume(false)
        }
        defer { timer.cancel() }
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                resume(false)
                pausing = continuation
            }
        } onCancel: {
            Task { @MainActor [weak self] in self?.resume(false) }
        }
    }

    /// The store saw the terminal of this link come back.
    func reachable() {
        if pausing == nil {
            reachedWhileBusy = true
        }
        resume(true)
    }

    private func resume(_ reached: Bool) {
        pausing?.resume(returning: reached)
        pausing = nil
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

    func receive(_ event: RuntimeEvent) {
        guard let pipe, event.string("pipe") == pipe else { return }
        guard event.type == "document.frame", (try? event.value("sequence", as: UInt64.self)) == next,
              let more = try? event.value("more", as: Bool.self), let data = event.string("data").flatMap({ Data(base64Encoded: $0) })
        else {
            lost()
            return
        }
        next += 1
        filling.append(data)
        if !more {
            onFrame(filling)
            filling = Data()
        }
    }

    private func lost() {
        pipe = nil
        onLost()
    }
}
