import Foundation

final class RuntimeEventInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var queue: [Data] = []
    private var byteCount = 0
    private var waiter: CheckedContinuation<Data?, Never>?
    private var closed = false
    private var overflowed = false
    private let maximumBytes: Int
    private let maximumCount: Int

    init(maximumBytes: Int = 16 * 1024 * 1024, maximumCount: Int = 256) {
        self.maximumBytes = maximumBytes
        self.maximumCount = maximumCount
    }

    var didOverflow: Bool {
        lock.withLock { overflowed }
    }

    func reopen() {
        lock.withLock {
            precondition(waiter == nil && queue.isEmpty)
            closed = false
            overflowed = false
        }
    }

    func push(_ data: Data) -> Bool {
        var waiting: CheckedContinuation<Data?, Never>?
        let accepted = lock.withLock {
            guard !closed, data.count <= maximumBytes else { return false }
            if let current = waiter {
                waiting = current
                waiter = nil
                return true
            }
            guard queue.count < maximumCount, byteCount <= maximumBytes - data.count else { return false }
            queue.append(data)
            byteCount += data.count
            return true
        }
        waiting?.resume(returning: data)
        return accepted
    }

    func next() async -> Data? {
        await withCheckedContinuation { continuation in
            var value: Data?
            var finished = false
            lock.withLock {
                if !queue.isEmpty {
                    value = queue.removeFirst()
                    byteCount -= value?.count ?? 0
                } else if closed {
                    finished = true
                } else {
                    precondition(waiter == nil, "one runtime event consumer")
                    waiter = continuation
                }
            }
            if let value {
                continuation.resume(returning: value)
            } else if finished {
                continuation.resume(returning: nil)
            }
        }
    }

    func close(overflowed: Bool = false) {
        let waiting = lock.withLock {
            closed = true
            self.overflowed = self.overflowed || overflowed
            queue.removeAll()
            byteCount = 0
            let current = waiter
            waiter = nil
            return current
        }
        waiting?.resume(returning: nil)
    }
}
