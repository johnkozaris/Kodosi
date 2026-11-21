import Foundation
import Synchronization

/// A full inbox drops all that waits rather than grow without end, and says so before the next
/// data, so that its reader can ask for the state again.
public final class RuntimeEventInbox: Sendable {
    public enum Arrival: Equatable, Sendable {
        case data(Data)
        /// The inbox dropped these, as its reader did not keep up.
        case behind(events: Int, bytes: Int)
    }

    public enum Item: Sendable {
        case event(RuntimeEvent)
        case unreadable(any Error)
        case behind(events: Int, bytes: Int)
    }

    private struct State {
        var queue: [Data] = []
        var byteCount = 0
        var waiter: CheckedContinuation<Arrival?, Never>?
        var closed = false
        var dropped: (events: Int, bytes: Int)?
    }

    private let state = Mutex(State())
    private let maximumBytes: Int
    private let maximumCount: Int
    private let decoder = JSONDecoder()

    /// The default holds a file of the canvas at its largest, in the parts of one frame, while the
    /// app reads them.
    public init(maximumBytes: Int = 64 * 1024 * 1024, maximumCount: Int = 256) {
        self.maximumBytes = maximumBytes
        self.maximumCount = maximumCount
    }

    public func reopen() {
        state.withLock {
            precondition($0.waiter == nil && $0.queue.isEmpty)
            $0.closed = false
            $0.dropped = nil
        }
    }

    /// Returns false when the inbox is closed, or when it was full and dropped all that waited.
    @discardableResult
    public func push(_ data: Data) -> Bool {
        var waiting: CheckedContinuation<Arrival?, Never>?
        let accepted = state.withLock { state in
            guard !state.closed else { return false }
            if let current = state.waiter {
                waiting = current
                state.waiter = nil
                return true
            }
            if state.queue.count < maximumCount, state.byteCount <= maximumBytes - data.count {
                state.queue.append(data)
                state.byteCount += data.count
                return true
            }
            let earlier = state.dropped ?? (events: 0, bytes: 0)
            state.dropped = (earlier.events + state.queue.count + 1, earlier.bytes + state.byteCount + data.count)
            state.queue.removeAll()
            state.byteCount = 0
            return false
        }
        waiting?.resume(returning: .data(data))
        return accepted
    }

    public func next() async -> Arrival? {
        await withCheckedContinuation { continuation in
            let ready: Arrival?? = state.withLock { state in
                if let dropped = state.dropped {
                    state.dropped = nil
                    return .behind(events: dropped.events, bytes: dropped.bytes)
                }
                if !state.queue.isEmpty {
                    let data = state.queue.removeFirst()
                    state.byteCount -= data.count
                    return .data(data)
                }
                if state.closed {
                    return .some(nil)
                }
                precondition(state.waiter == nil, "one runtime event consumer")
                state.waiter = continuation
                return nil
            }
            if let ready {
                continuation.resume(returning: ready)
            }
        }
    }

    @concurrent
    public func nextItem() async -> Item? {
        switch await next() {
        case nil:
            return nil
        case let .behind(events, bytes):
            return .behind(events: events, bytes: bytes)
        case let .data(data):
            do {
                return try .event(decoder.decode(RuntimeEvent.self, from: data))
            } catch {
                return .unreadable(error)
            }
        }
    }

    public func close() {
        let waiting = state.withLock {
            $0.closed = true
            $0.dropped = nil
            $0.queue.removeAll()
            $0.byteCount = 0
            let current = $0.waiter
            $0.waiter = nil
            return current
        }
        waiting?.resume(returning: nil)
    }
}
