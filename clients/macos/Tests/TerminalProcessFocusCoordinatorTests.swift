import Foundation
@testable import KodosiDesktop
import Testing

@Test @MainActor
func rejectedFocusRetriesOnceThenRevokesOnlyTheCurrentLease() {
    var requests: [(TerminalProcessFocusLease, String)] = []
    var revoked = 0
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { lease, request, completion in requests.append((lease, request)); completion(true) },
        sendBlur: { _, completion in completion(true) }
    )
    let surface = UUID()
    coordinator.registerRevoker(surfaceId: surface) { revoked += 1 }
    coordinator.acquire(sessionId: "session", runtimeIncarnationId: "current", subscriptionId: "subscription",
                        subscriptionGeneration: 1, surfaceId: surface)
    coordinator.applyResult(sessionId: "session", runtimeIncarnationId: "old", requestId: requests[0].1, accepted: false)
    #expect(requests.count == 1)
    coordinator.applyResult(sessionId: "session", runtimeIncarnationId: "current", requestId: requests[0].1, accepted: false)
    #expect(requests.count == 2)
    coordinator.applyResult(sessionId: "session", runtimeIncarnationId: "current", requestId: requests[0].1, accepted: true)
    coordinator.applyResult(sessionId: "session", runtimeIncarnationId: "current", requestId: requests[1].1, accepted: false)
    #expect(revoked == 1)
    coordinator.acquire(sessionId: "session", runtimeIncarnationId: "current", subscriptionId: "subscription",
                        subscriptionGeneration: 1, surfaceId: surface)
    #expect(requests.count == 3)
}

@Test @MainActor
func acceptedPendingTerminalBlurRefocusesReacquiredLease() {
    var focusLeases: [TerminalProcessFocusLease] = []
    var blurAuthorities: [TerminalProcessFocusReleaseAuthority] = []
    var blurCompletions: [(Bool) -> Void] = []
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { lease, _, completion in
            focusLeases.append(lease)
            completion(true)
        },
        sendBlur: { authority, completion in
            blurAuthorities.append(authority)
            blurCompletions.append(completion)
        }
    )
    let lease = TerminalProcessFocusLease(
        sessionId: "session",
        runtimeIncarnationId: "incarnation",
        subscriptionId: "subscription", subscriptionGeneration: 1
    )
    let surfaceId = UUID()

    coordinator.acquire(
        sessionId: lease.sessionId,
        runtimeIncarnationId: lease.runtimeIncarnationId,
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )
    coordinator.release(surfaceId: surfaceId)
    coordinator.acquire(
        sessionId: lease.sessionId,
        runtimeIncarnationId: lease.runtimeIncarnationId,
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )

    #expect(focusLeases == [lease])
    #expect(blurAuthorities.count == 1)
    #expect(blurAuthorities[0].lease == lease)
    #expect(blurAuthorities[0].generation == 1)
    #expect(!blurAuthorities[0].requestId.isEmpty)

    blurCompletions[0](true)

    #expect(focusLeases == [lease, lease])
    coordinator.release(surfaceId: surfaceId)
    #expect(blurAuthorities.map(\.generation) == [1, 2])
}

@Test @MainActor
func terminallyRejectedBlurRetriesWithoutReacquisition() {
    var blurAuthorities: [TerminalProcessFocusReleaseAuthority] = []
    var blurCompletions: [(Bool) -> Void] = []
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { _, _, completion in completion(true) },
        sendBlur: { authority, completion in
            blurAuthorities.append(authority)
            blurCompletions.append(completion)
        }
    )
    let surfaceId = UUID()

    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "incarnation",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )
    coordinator.release(surfaceId: surfaceId)

    blurCompletions[0](false)

    #expect(blurAuthorities.count == 2)
    #expect(blurAuthorities.map(\.generation) == [1, 1])
    #expect(blurAuthorities[0].requestId != blurAuthorities[1].requestId)

    blurCompletions[1](false)
    #expect(blurAuthorities.count == 2)

    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "incarnation",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )
    #expect(blurAuthorities.count == 2)
}

@Test @MainActor
func staleBlurCallbackCannotDisplaceReplacementBlurGeneration() {
    var focusCount = 0
    var blurAuthorities: [TerminalProcessFocusReleaseAuthority] = []
    var blurCompletions: [(Bool) -> Void] = []
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { _, _, completion in
            focusCount += 1
            completion(true)
        },
        sendBlur: { authority, completion in
            blurAuthorities.append(authority)
            blurCompletions.append(completion)
        }
    )
    let surfaceId = UUID()

    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "incarnation",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )
    coordinator.release(surfaceId: surfaceId)
    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "incarnation",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )
    coordinator.release(surfaceId: surfaceId)

    #expect(focusCount == 1)
    #expect(blurAuthorities.map(\.generation) == [1, 2])
    blurCompletions[0](false)
    #expect(blurAuthorities.map(\.generation) == [1, 2])
    blurCompletions[1](true)
}

@Test @MainActor
func retiredIncarnationBlurRetriesCannotRefocusReplacementIncarnation() {
    var focusLeases: [TerminalProcessFocusLease] = []
    var blurCalls: [(TerminalProcessFocusReleaseAuthority, (Bool) -> Void)] = []
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { lease, _, completion in
            focusLeases.append(lease)
            completion(true)
        },
        sendBlur: { authority, completion in
            blurCalls.append((authority, completion))
        }
    )
    let surfaceId = UUID()

    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "old",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )
    coordinator.release(surfaceId: surfaceId)
    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "replacement",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId
    )

    blurCalls[0].1(false)

    #expect(blurCalls.map(\.0.lease.runtimeIncarnationId) == ["old", "old"])
    #expect(focusLeases.map(\.runtimeIncarnationId) == ["old", "replacement"])
    blurCalls[1].1(false)
    coordinator.release(surfaceId: surfaceId)
    #expect(blurCalls.map(\.0.lease.runtimeIncarnationId) == ["old", "old", "replacement"])
}
