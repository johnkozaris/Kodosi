import Foundation
import KodosiTerminalSessions
import Testing

@Test
func `rejected focus retries once then revokes only the current lease`() {
    var requests: [(TerminalProcessFocusLease, String)] = []
    var revoked = 0
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { lease, request, _, completion in requests.append((lease, request)); completion(true) },
        sendBlur: { _, completion in completion(true) },
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

@Test
func `retired incarnation blur retries cannot refocus replacement incarnation`() {
    var focusLeases: [TerminalProcessFocusLease] = []
    var blurCalls: [(TerminalProcessFocusReleaseAuthority, (Bool) -> Void)] = []
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { lease, _, _, completion in
            focusLeases.append(lease)
            completion(true)
        },
        sendBlur: { authority, completion in
            blurCalls.append((authority, completion))
        },
    )
    let surfaceId = UUID()

    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "old",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId,
    )
    coordinator.release(surfaceId: surfaceId)
    coordinator.acquire(
        sessionId: "session",
        runtimeIncarnationId: "replacement",
        subscriptionId: "subscription", subscriptionGeneration: 1,
        surfaceId: surfaceId,
    )

    blurCalls[0].1(false)

    #expect(blurCalls.map(\.0.lease.runtimeIncarnationId) == ["old", "old"])
    #expect(focusLeases.map(\.runtimeIncarnationId) == ["old", "replacement"])
    blurCalls[1].1(false)
    coordinator.release(surfaceId: surfaceId)
    #expect(blurCalls.map(\.0.lease.runtimeIncarnationId) == ["old", "old", "replacement"])
}

@Test
func `a focus that comes back goes again as a person's act when they act on its view`() {
    var focuses: [(request: String, returning: Bool)] = []
    let coordinator = TerminalProcessFocusCoordinator(
        sendFocus: { _, request, returning, completion in focuses.append((request, returning)); completion(true) },
        sendBlur: { _, completion in completion(true) },
    )
    let surface = UUID()
    coordinator.acquire(sessionId: "session", runtimeIncarnationId: "current", subscriptionId: "subscription",
                        subscriptionGeneration: 1, surfaceId: surface, returning: true)
    coordinator.applyResult(sessionId: "session", runtimeIncarnationId: "current", requestId: focuses[0].request,
                            accepted: true)
    coordinator.act(surfaceId: surface)
    #expect(focuses.map(\.returning) == [true, false])

    let other = UUID()
    coordinator.acquire(sessionId: "other", runtimeIncarnationId: "current", subscriptionId: "other-subscription",
                        subscriptionGeneration: 1, surfaceId: other, returning: true)
    coordinator.act(surfaceId: other)
    #expect(focuses.map(\.returning) == [true, false, true])
    coordinator.applyResult(sessionId: "other", runtimeIncarnationId: "current", requestId: focuses[2].request,
                            accepted: true)
    #expect(focuses.map(\.returning) == [true, false, true, false])
}
