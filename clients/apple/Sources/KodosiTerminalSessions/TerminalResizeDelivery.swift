import Foundation
import KodosiBridge
import Synchronization

public final class ResizeDeliveryState: Sendable {
    private struct PendingResize: Sendable {
        let key: String
        let request: TerminalResizeIdentity
        let admittedAtNanoseconds: UInt64
    }

    private struct State: Sendable {
        var applied = ""
        var pending: [String: PendingResize] = [:]
    }

    private static let pendingLeaseNanoseconds: UInt64 = 5_000_000_000

    private let state = Mutex(State())

    public init() {}

    public func begin(_ request: TerminalResizeIdentity, key: String) -> TerminalResizeIdentity? {
        let nowNanoseconds = DispatchTime.now().uptimeNanoseconds
        return state.withLock {
            $0.pending = $0.pending.filter {
                nowNanoseconds &- $0.value.admittedAtNanoseconds < Self.pendingLeaseNanoseconds
            }
            let latest = $0.pending.values.max { $0.admittedAtNanoseconds < $1.admittedAtNanoseconds }
            guard latest?.key != key,
                  latest != nil || key != $0.applied
            else { return nil }
            $0.pending[request.requestId] = PendingResize(
                key: key,
                request: request,
                admittedAtNanoseconds: nowNanoseconds
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
