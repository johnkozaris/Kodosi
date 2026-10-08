import Foundation
import Synchronization

final class ResizeDeliveryState: Sendable {
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

    func begin(
        _ request: TerminalResizeIdentity,
        key: String
    ) -> TerminalResizeIdentity? {
        begin(
            request,
            key: key,
            nowNanoseconds: DispatchTime.now().uptimeNanoseconds
        )
    }

    private func begin(
        _ request: TerminalResizeIdentity,
        key: String,
        nowNanoseconds: UInt64
    ) -> TerminalResizeIdentity? {
        state.withLock {
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

    func queued(_ request: TerminalResizeIdentity, accepted: Bool) {
        guard !accepted else { return }
        _ = reject(request)
    }

    @discardableResult
    func apply(_ result: TerminalResizeIdentity) -> Bool {
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
    func reject(_ result: TerminalResizeIdentity) -> Bool {
        state.withLock {
            guard let pending = $0.pending[result.requestId],
                  pending.request == result
            else { return false }
            $0.pending.removeValue(forKey: result.requestId)
            return true
        }
    }

    func reset() {
        state.withLock {
            $0.applied = ""
            $0.pending.removeAll()
        }
    }
}
