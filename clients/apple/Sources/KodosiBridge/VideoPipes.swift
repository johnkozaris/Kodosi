import Foundation
import Synchronization

/// Routes the events of video pipes (the shared browser's and the screen's) to a dedicated inbox
/// and background task so video frames never block behind main-actor events or main-thread work.
public final class VideoPipes: Sendable {
    private struct Claim {
        let mark: Data
        weak var link: DocumentLink?
    }

    private struct State {
        var claims: [String: Claim] = [:]
        var reading = false
    }

    private let state = Mutex(State())
    private let inbox = RuntimeEventInbox()

    public init() {}

    deinit {
        inbox.close()
    }

    /// Claims and enqueues events for active video pipes. Raw byte matching is safe without JSON
    /// parsing because only pipe events carry `"pipe":"..."` and base64 payloads contain no quotes.
    public func take(_ event: Data) -> Bool {
        let marks = state.withLock { $0.claims.isEmpty ? [] : $0.claims.values.map(\.mark) }
        guard marks.contains(where: { Self.names(event, $0) }) else { return false }
        inbox.push(event)
        return true
    }

    func claim(_ pipe: String, for link: DocumentLink) {
        let starts = state.withLock { state in
            state.claims[pipe] = Claim(mark: Data(#""pipe":"\#(pipe)""#.utf8), link: link)
            defer { state.reading = true }
            return !state.reading
        }
        if starts {
            read()
        }
    }

    func release(_ pipe: String) {
        _ = state.withLock { $0.claims.removeValue(forKey: pipe) }
    }

    private func read() {
        let inbox = inbox
        Task.detached(name: "Video frames", priority: .userInitiated) { [weak self] in
            var faults = RuntimeEventFaults()
            while let item = await inbox.nextItem() {
                guard let self else { return }
                switch item {
                case let .event(event):
                    let pipe = event.string("pipe")
                    state.withLock { state in pipe.flatMap { state.claims[$0]?.link } }?.receive(event)
                case let .unreadable(error):
                    faults.unreadable(nil, error)
                case let .behind(drop):
                    RuntimeEventFaults.videoBehind(drop)
                    for link in state.withLock({ $0.claims.values.compactMap(\.link) }) {
                        link.renewAfterLoss()
                    }
                }
            }
        }
    }

    private static func names(_ event: Data, _ mark: Data) -> Bool {
        event.withUnsafeBytes { event in
            mark.withUnsafeBytes { mark in
                memmem(event.baseAddress, event.count, mark.baseAddress, mark.count) != nil
            }
        }
    }
}
