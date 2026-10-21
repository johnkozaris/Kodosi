import Foundation
import KodosiBridge

public struct TerminalProcessFocusLease: Hashable, Sendable {
    public let sessionId: String
    public let runtimeIncarnationId: String
    public let subscriptionId: String
    public let subscriptionGeneration: UInt64

    public init(sessionId: String, runtimeIncarnationId: String, subscriptionId: String, subscriptionGeneration: UInt64) {
        self.sessionId = sessionId
        self.runtimeIncarnationId = runtimeIncarnationId
        self.subscriptionId = subscriptionId
        self.subscriptionGeneration = subscriptionGeneration
    }
}

public struct TerminalProcessFocusReleaseAuthority: Equatable, Sendable {
    public let lease: TerminalProcessFocusLease
    public let generation: UInt64
    public let requestId: String
}

@MainActor
public final class TerminalProcessFocusCoordinator {
    private struct PendingFocus: Equatable {
        let generation: UInt64
        let requestId: String
    }

    private struct LeaseState {
        var surfaces: Set<UUID> = []
        var generation: UInt64?
        var focus: PendingFocus?
        var release: TerminalProcessFocusReleaseAuthority?
        var focusRetries = 0
    }

    private let sendFocus: (TerminalProcessFocusLease, String, @escaping (Bool) -> Void) -> Void
    private let sendBlur: (TerminalProcessFocusReleaseAuthority, @escaping (Bool) -> Void) -> Void
    private var leases: [TerminalProcessFocusLease: LeaseState] = [:]
    private var leaseBySurfaceId: [UUID: TerminalProcessFocusLease] = [:]
    private var revokerBySurfaceId: [UUID: @MainActor () -> Void] = [:]
    private var lastGeneration: UInt64 = 0

    public init(
        sendFocus: @escaping (TerminalProcessFocusLease, String, @escaping (Bool) -> Void) -> Void,
        sendBlur: @escaping (TerminalProcessFocusReleaseAuthority, @escaping (Bool) -> Void) -> Void,
    ) {
        self.sendFocus = sendFocus
        self.sendBlur = sendBlur
    }

    public func registerRevoker(surfaceId: UUID, revoker: (@MainActor () -> Void)?) {
        revokerBySurfaceId[surfaceId] = revoker
    }

    public func acquire(
        sessionId: String,
        runtimeIncarnationId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64,
        surfaceId: UUID,
    ) {
        let lease = TerminalProcessFocusLease(
            sessionId: sessionId,
            runtimeIncarnationId: runtimeIncarnationId,
            subscriptionId: subscriptionId,
            subscriptionGeneration: subscriptionGeneration,
        )
        guard leaseBySurfaceId[surfaceId] != lease else { return }
        release(surfaceId: surfaceId)
        var state = leases[lease] ?? LeaseState()
        let wasFocused = !state.surfaces.isEmpty
        state.surfaces.insert(surfaceId)
        leaseBySurfaceId[surfaceId] = lease
        if !wasFocused {
            precondition(lastGeneration < UInt64.max, "terminal focus generation exhausted")
            lastGeneration += 1
            state.generation = lastGeneration
            state.focusRetries = 0
        }
        leases[lease] = state
        if !wasFocused, state.release == nil, let generation = state.generation {
            requestFocus(lease, generation: generation)
        }
    }

    public func applyResult(sessionId: String, runtimeIncarnationId: String, requestId: String, accepted: Bool) {
        guard let (lease, state) = leases.first(where: {
            $0.key.sessionId == sessionId && $0.key.runtimeIncarnationId == runtimeIncarnationId
                && $0.value.focus?.requestId == requestId
        }), let pending = state.focus, state.generation == pending.generation else { return }
        leases[lease]?.focus = nil
        if accepted {
            leases[lease]?.focusRetries = 0
        } else if !state.surfaces.isEmpty, state.focusRetries == 0 {
            leases[lease]?.focusRetries = 1
            requestFocus(lease, generation: pending.generation)
        } else {
            revoke(lease, generation: pending.generation)
        }
    }

    public func releaseSession(sessionId: String) {
        for (surface, lease) in leaseBySurfaceId where lease.sessionId == sessionId {
            release(surfaceId: surface)
        }
    }

    public func release(surfaceId: UUID) {
        guard let lease = leaseBySurfaceId.removeValue(forKey: surfaceId),
              var state = leases[lease] else { return }
        state.surfaces.remove(surfaceId)
        let generation = state.generation
        if state.surfaces.isEmpty {
            state.generation = nil
            state.focus = nil
            state.focusRetries = 0
        }
        leases[lease] = state
        if state.surfaces.isEmpty, let generation {
            requestBlur(lease, generation: generation, retryCount: 0)
        }
    }

    private func requestFocus(_ lease: TerminalProcessFocusLease, generation: UInt64) {
        guard let state = leases[lease], state.generation == generation, !state.surfaces.isEmpty else { return }
        let pending = PendingFocus(generation: generation, requestId: UUIDv7.generate())
        leases[lease]?.focus = pending
        sendFocus(lease, pending.requestId) { [weak self] admitted in
            guard !admitted, let self, leases[lease]?.focus == pending else { return }
            revoke(lease, generation: generation)
        }
    }

    private func requestBlur(_ lease: TerminalProcessFocusLease, generation: UInt64, retryCount: Int) {
        let authority = TerminalProcessFocusReleaseAuthority(
            lease: lease, generation: generation, requestId: UUIDv7.generate(),
        )
        leases[lease]?.release = authority
        sendBlur(authority) { [weak self] admitted in
            guard let self, leases[lease]?.release == authority else { return }
            if !admitted, retryCount < 1 {
                requestBlur(lease, generation: generation, retryCount: retryCount + 1)
                return
            }
            leases[lease]?.release = nil
            if admitted, let state = leases[lease], let active = state.generation, !state.surfaces.isEmpty {
                leases[lease]?.focusRetries = 0
                requestFocus(lease, generation: active)
            }
            prune(lease)
        }
    }

    private func revoke(_ lease: TerminalProcessFocusLease, generation: UInt64) {
        guard let state = leases[lease], state.generation == generation else { return }
        leases[lease]?.focus = nil
        leases[lease]?.generation = nil
        leases[lease]?.focusRetries = 0
        leases[lease]?.surfaces.removeAll()
        for surface in state.surfaces where leaseBySurfaceId[surface] == lease {
            leaseBySurfaceId.removeValue(forKey: surface)
            revokerBySurfaceId[surface]?()
        }
        prune(lease)
    }

    private func prune(_ lease: TerminalProcessFocusLease) {
        if let state = leases[lease], state.surfaces.isEmpty, state.release == nil {
            leases.removeValue(forKey: lease)
        }
    }
}

public extension TerminalProcessFocusCoordinator {
    convenience init(commands: CommandSink) {
        self.init(
            sendFocus: { lease, requestId, completion in
                completion(CommandSink.didAcceptSynchronously(commands.sendTerminalFocus(
                    sessionId: lease.sessionId, clientId: lease.subscriptionId,
                    subscriptionGeneration: lease.subscriptionGeneration,
                    requestId: requestId, expectedRuntimeIncarnationId: lease.runtimeIncarnationId,
                )))
            },
            sendBlur: { authority, completion in
                completion(CommandSink.didAcceptSynchronously(commands.sendTerminalBlur(
                    sessionId: authority.lease.sessionId, clientId: authority.lease.subscriptionId,
                    subscriptionGeneration: authority.lease.subscriptionGeneration,
                    expectedRuntimeIncarnationId: authority.lease.runtimeIncarnationId,
                )))
            },
        )
    }
}
