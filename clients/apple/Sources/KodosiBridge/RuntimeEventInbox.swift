import Foundation
import Synchronization

/// A full inbox drops all that waits rather than grow without end, and says so before the next
/// data, so that its reader can ask for the state again.
public final class RuntimeEventInbox: Sendable {
    public enum Arrival: Equatable, Sendable {
        case data(Data)
        /// The inbox dropped these, as its reader did not keep up.
        case behind(Drop)
    }

    public enum Item: Sendable {
        case event(RuntimeEvent)
        case unreadable(any Error)
        case behind(Drop)
    }

    /// What a full inbox dropped, so that the log can say what filled it.
    public struct Drop: Equatable, Sendable {
        public var events = 0
        public var bytes = 0
        /// How long the reader had not caught up.
        public var behindFor = Duration.zero
        /// The kinds of events with the most bytes, most first.
        public var kinds = ""
    }

    private struct State {
        /// An array moves all that follow when its first item goes, so the newest events go on the
        /// end of one array and the oldest come off the end of another.
        var incoming: [Data] = []
        var outgoing: [Data] = []
        var byteCount = 0
        var filling: ContinuousClock.Instant?
        var waiter: CheckedContinuation<Arrival?, Never>?
        var closed = false
        var dropped: Drop?
        var droppedKinds: [String: (events: Int, bytes: Int)] = [:]

        var count: Int {
            incoming.count + outgoing.count
        }

        var isEmpty: Bool {
            incoming.isEmpty && outgoing.isEmpty
        }
    }

    private let state = Mutex(State())
    private let maximumBytes: Int
    private let maximumCount: Int
    private let decoder = JSONDecoder()

    /// The default holds a file of the canvas at its largest, in the parts of one frame, and the
    /// many small events that can come at once, while the app reads them.
    public init(maximumBytes: Int = 64 * 1024 * 1024, maximumCount: Int = 4096) {
        self.maximumBytes = maximumBytes
        self.maximumCount = maximumCount
    }

    public func reopen() {
        state.withLock {
            precondition($0.waiter == nil && $0.isEmpty)
            $0.closed = false
            $0.dropped = nil
            $0.droppedKinds = [:]
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
            if state.count < maximumCount, state.byteCount <= maximumBytes - data.count {
                if state.isEmpty {
                    state.filling = .now
                }
                state.incoming.append(data)
                state.byteCount += data.count
                return true
            }
            var drop = state.dropped ?? Drop()
            drop.events += state.count + 1
            drop.bytes += state.byteCount + data.count
            drop.behindFor = max(drop.behindFor, state.filling.map { .now - $0 } ?? .zero)
            for dropped in state.outgoing + state.incoming + [data] {
                let kind = Self.kind(of: dropped)
                state.droppedKinds[kind, default: (0, 0)].events += 1
                state.droppedKinds[kind, default: (0, 0)].bytes += dropped.count
            }
            state.dropped = drop
            state.incoming.removeAll()
            state.outgoing.removeAll()
            state.byteCount = 0
            state.filling = nil
            return false
        }
        waiting?.resume(returning: .data(data))
        return accepted
    }

    public func next() async -> Arrival? {
        await withCheckedContinuation { continuation in
            let ready: Arrival?? = state.withLock { state in
                if var dropped = state.dropped {
                    dropped.kinds = state.droppedKinds.sorted { $0.value.bytes > $1.value.bytes }.prefix(4)
                        .map { "\($0.key) \($0.value.events) in \($0.value.bytes / 1024) KB" }
                        .joined(separator: ", ")
                    state.dropped = nil
                    state.droppedKinds = [:]
                    return .behind(dropped)
                }
                if state.outgoing.isEmpty {
                    state.outgoing = state.incoming.reversed()
                    state.incoming.removeAll(keepingCapacity: true)
                }
                if let data = state.outgoing.popLast() {
                    state.byteCount -= data.count
                    if state.isEmpty {
                        state.filling = nil
                    }
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
        case let .behind(drop):
            return .behind(drop)
        case let .data(data):
            do {
                return try .event(decoder.decode(RuntimeEvent.self, from: data))
            } catch {
                return .unreadable(error)
            }
        }
    }

    /// The runtime writes the keys of an event in order, and a long event keeps its type after
    /// its data, so the type is near one of its ends.
    private static func kind(of data: Data) -> String {
        for end in [data.suffix(256), data.prefix(256)] {
            let text = String(decoding: end, as: UTF8.self)
            if let start = text.range(of: #""type":""#) {
                return String(text[start.upperBound...].prefix { $0 != "\"" })
            }
        }
        return "unknown"
    }

    public func close() {
        let waiting = state.withLock {
            $0.closed = true
            $0.dropped = nil
            $0.droppedKinds = [:]
            $0.incoming.removeAll()
            $0.outgoing.removeAll()
            $0.byteCount = 0
            $0.filling = nil
            let current = $0.waiter
            $0.waiter = nil
            return current
        }
        waiting?.resume(returning: nil)
    }
}
