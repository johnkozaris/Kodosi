import Foundation
import Synchronization

public final class RuntimeEventInbox: Sendable {
    private struct State {
        var queue: [Data] = []
        var byteCount = 0
        var waiter: CheckedContinuation<Data?, Never>?
        var closed = false
        var overflowed = false
    }

    private let state = Mutex(State())
    private let maximumBytes: Int
    private let maximumCount: Int
    private let decoder = JSONDecoder()

    public init(maximumBytes: Int = 16 * 1024 * 1024, maximumCount: Int = 256) {
        self.maximumBytes = maximumBytes
        self.maximumCount = maximumCount
    }

    public var didOverflow: Bool {
        state.withLock { $0.overflowed }
    }

    public func reopen() {
        state.withLock {
            precondition($0.waiter == nil && $0.queue.isEmpty)
            $0.closed = false
            $0.overflowed = false
        }
    }

    public func push(_ data: Data) -> Bool {
        var waiting: CheckedContinuation<Data?, Never>?
        let accepted = state.withLock { state in
            guard !state.closed, data.count <= maximumBytes else { return false }
            if let current = state.waiter {
                waiting = current
                state.waiter = nil
                return true
            }
            guard state.queue.count < maximumCount, state.byteCount <= maximumBytes - data.count else { return false }
            state.queue.append(data)
            state.byteCount += data.count
            return true
        }
        waiting?.resume(returning: data)
        return accepted
    }

    public func next() async -> Data? {
        await withCheckedContinuation { continuation in
            var value: Data?
            var finished = false
            state.withLock { state in
                if !state.queue.isEmpty {
                    value = state.queue.removeFirst()
                    state.byteCount -= value?.count ?? 0
                } else if state.closed {
                    finished = true
                } else {
                    precondition(state.waiter == nil, "one runtime event consumer")
                    state.waiter = continuation
                }
            }
            if let value {
                continuation.resume(returning: value)
            } else if finished {
                continuation.resume(returning: nil)
            }
        }
    }

    @concurrent
    public func nextEvent() async -> Result<RuntimeEvent, any Error>? {
        guard let data = await next() else { return nil }
        return Result { try decoder.decode(RuntimeEvent.self, from: data) }
    }

    public func close(overflowed: Bool = false) {
        let waiting = state.withLock {
            $0.closed = true
            $0.overflowed = $0.overflowed || overflowed
            $0.queue.removeAll()
            $0.byteCount = 0
            let current = $0.waiter
            $0.waiter = nil
            return current
        }
        waiting?.resume(returning: nil)
    }
}
