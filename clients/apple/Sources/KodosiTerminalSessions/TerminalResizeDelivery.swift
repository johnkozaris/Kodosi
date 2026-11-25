import Foundation
import KodosiBridge
import Synchronization

public final class ResizeDeliveryState: Sendable {
    private struct PendingResize: Sendable {
        let key: String
        let request: TerminalResizeIdentity
        let admittedAt: SuspendingClock.Instant
    }

    private struct State: Sendable {
        var applied = ""
        var pending: [String: PendingResize] = [:]
    }

    private static let pendingLease = Duration.seconds(5)

    private let state = Mutex(State())

    public init() {}

    public func begin(_ request: TerminalResizeIdentity, key: String) -> TerminalResizeIdentity? {
        let now = SuspendingClock.now
        return state.withLock {
            $0.pending = $0.pending.filter {
                $0.value.admittedAt.duration(to: now) < Self.pendingLease
            }
            let latest = $0.pending.values.max { $0.admittedAt < $1.admittedAt }
            guard latest?.key != key,
                  latest != nil || key != $0.applied
            else { return nil }
            $0.pending[request.requestId] = PendingResize(
                key: key,
                request: request,
                admittedAt: now,
            )
            return request
        }
    }

    public func queued(_ request: TerminalResizeIdentity, accepted: Bool) {
        guard !accepted else { return }
        _ = reject(request)
    }

    @discardableResult
    public func apply(_ result: TerminalResizeIdentity) -> Bool {
        state.withLock {
            guard let pending = $0.pending[result.requestId],
                  pending.request == result
            else { return false }
            $0.applied = pending.key
            $0.pending.removeValue(forKey: result.requestId)
            return true
        }
    }

    @discardableResult
    public func reject(_ result: TerminalResizeIdentity) -> Bool {
        state.withLock {
            guard let pending = $0.pending[result.requestId],
                  pending.request == result
            else { return false }
            // Another viewer may now own the terminal size, so clear `applied` to prevent
            // deduplicating a future resize back to this view's previous dimensions.
            $0.applied = ""
            $0.pending.removeValue(forKey: result.requestId)
            return true
        }
    }

    public func reset() {
        state.withLock {
            $0.applied = ""
            $0.pending.removeAll()
        }
    }
}

public final class TerminalResizeAuthorityCell: Sendable {
    private let value: Mutex<Bool>

    public init(_ value: Bool) {
        self.value = Mutex(value)
    }

    public var isAllowed: Bool {
        value.withLock { $0 }
    }

    public func update(_ newValue: Bool) {
        value.withLock { $0 = newValue }
    }
}
