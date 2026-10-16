import Foundation
import KodosiBridge
import KodosiTerminal
import Observation
import os

@MainActor
public final class ManagedTerminalSession {
    public let renderer: TerminalRendererSession
    public let token: TerminalConnectionToken
    public let resizeAuthority: TerminalResizeAuthorityCell
    public let resizeDelivery: ResizeDeliveryState
    public let runtimeIncarnationId: String
    var pendingResize: Task<Void, Never>?
    let controls = RuntimeEventInbox(maximumBytes: 4 * 1024 * 1024, maximumCount: 64)
    var controlTask: Task<Void, Never>?
    var runtimeConnected = false
    var restoredSurfaceGeneration: UInt64 = 0

    init(
        renderer: TerminalRendererSession,
        token: TerminalConnectionToken,
        resizeAuthority: TerminalResizeAuthorityCell,
        resizeDelivery: ResizeDeliveryState,
        runtimeIncarnationId: String
    ) {
        self.renderer = renderer
        self.token = token
        self.resizeAuthority = resizeAuthority
        self.resizeDelivery = resizeDelivery
        self.runtimeIncarnationId = runtimeIncarnationId
    }
}

public enum TerminalRenderState: Equatable, Sendable {
    case connecting
    case rendered
    case closed(String?)
    case failed(String)
}

@MainActor
@Observable
public final class TerminalSessionManager {
    public typealias Disconnect = @MainActor (
        _ sessionId: String,
        _ subscriptionId: String,
        _ subscriptionGeneration: UInt64
    ) -> Void

    private var inputErrorsBySession: [String: String] = [:]
    var renderStateBySession: [String: TerminalRenderState] = [:]
    var resizeNotesBySession: [String: String] = [:]
    private var runtimeIncarnationsBySession: [String: String] = [:]
    private var resizeAuthorityBySession: [String: Bool] = [:]

    let runtimeHandle: RuntimeHandle
    private let commandSink: CommandSink
    private let style: @MainActor () -> TerminalStyle
    private let claimsSizeByDefault: Bool
    @ObservationIgnored private var sizeClaims: Set<String> = []
    let beep: @MainActor () -> Void
    let onFocusResult: @MainActor (String, String, String, Bool) -> Void
    private let disconnectTerminal: Disconnect
    var sessions: [String: ManagedTerminalSession] = [:]

    public init(
        runtimeHandle: RuntimeHandle,
        commandSink: CommandSink,
        style: @escaping @MainActor () -> TerminalStyle,
        disconnectTerminal: Disconnect? = nil,
        claimsSizeByDefault: Bool = true,
        beep: @escaping @MainActor () -> Void = {},
        onFocusResult: @escaping @MainActor (String, String, String, Bool) -> Void = { _, _, _, _ in }
    ) {
        self.runtimeHandle = runtimeHandle
        self.commandSink = commandSink
        self.style = style
        self.disconnectTerminal = disconnectTerminal ?? { sessionId, subscriptionId, subscriptionGeneration in
            runtimeHandle.disconnectTerminal(
                sessionId: sessionId,
                subscriptionId: subscriptionId,
                subscriptionGeneration: subscriptionGeneration
            )
        }
        self.claimsSizeByDefault = claimsSizeByDefault
        self.beep = beep
        self.onFocusResult = onFocusResult
    }

    public func applyStyle(_ style: TerminalStyle) {
        for managed in sessions.values {
            managed.renderer.setStyle(style)
        }
    }

    public func session(for sessionId: String) -> ManagedTerminalSession {
        if let existing = sessions[sessionId] {
            return existing
        }
        return replaceSession(sessionId: sessionId)
    }

    private func reconcileSession(_ sessionId: String) {
        guard let existing = sessions[sessionId],
              !existing.token.isActive
        else { return }
        _ = replaceSession(sessionId: sessionId)
    }

    public func reconcile(with catalog: [RuntimeSession]) {
        let visible = catalog.filter { $0.connectionState != .blocked }
        reconcileSessions(
            liveSessionIds: Set(visible.map(\.id)),
            runtimeIncarnations: Dictionary(uniqueKeysWithValues: visible.map { ($0.id, $0.incarnationId) }),
            resizeAuthority: Dictionary(uniqueKeysWithValues: visible.map { ($0.id, $0.canControl) })
        )
    }

    public func reconcileSessions(
        liveSessionIds: Set<String>,
        runtimeIncarnations: [String: String] = [:],
        resizeAuthority: [String: Bool] = [:]
    ) {
        let replacedSessionIds: Set<String> = Set(sessions.compactMap { sessionId, managed -> String? in
            guard let incarnation = runtimeIncarnations[sessionId],
                  incarnation != managed.runtimeIncarnationId
            else { return nil }
            return sessionId
        })
        runtimeIncarnationsBySession = runtimeIncarnations
        let previousResizeAuthority = resizeAuthorityBySession
        resizeAuthorityBySession = resizeAuthority
        for (sessionId, managed) in sessions {
            let isAllowed = resizeAuthority[sessionId] == true
            managed.resizeAuthority.update(isAllowed)
            if !isAllowed || previousResizeAuthority[sessionId] != isAllowed {
                managed.resizeDelivery.reset()
            }
        }
        for sessionId in replacedSessionIds {
            releaseSession(for: sessionId)
        }

        let releasedSessionIds = Set(sessions.keys).subtracting(liveSessionIds)
        for sessionId in releasedSessionIds {
            releaseSession(for: sessionId)
        }
        for sessionId in liveSessionIds {
            reconcileSession(sessionId)
        }
    }

    private func replaceSession(sessionId: String) -> ManagedTerminalSession {
        let previous = sessions.removeValue(forKey: sessionId)
        previous?.pendingResize?.cancel()
        previous?.controls.close()
        previous?.controlTask?.cancel()
        previous?.resizeDelivery.reset()
        previous?.token.deactivate()
        resizeNotesBySession.removeValue(forKey: sessionId)
        TerminalInputQueue.shared.cancel(sessionId: sessionId)
        if let previous {
            disconnectTerminal(
                sessionId,
                previous.token.subscriptionId,
                previous.token.subscriptionGeneration
            )
        }
        let managed = createSession(sessionId: sessionId)
        sessions[sessionId] = managed
        return managed
    }

    func refreshRendererSession(
        sessionId: String,
        expectedToken: TerminalConnectionToken
    ) {
        guard let existing = sessions[sessionId],
              existing.token === expectedToken,
              existing.token.isActive
        else { return }
        runtimeHandle.refreshTerminal(
            sessionId: sessionId,
            subscriptionId: existing.token.subscriptionId,
            subscriptionGeneration: existing.token.subscriptionGeneration
        )
    }

    public func releaseSession(for sessionId: String) {
        let existing = sessions.removeValue(forKey: sessionId)
        existing?.pendingResize?.cancel()
        existing?.controls.close()
        existing?.controlTask?.cancel()
        existing?.resizeDelivery.reset()
        existing?.token.deactivate()
        inputErrorsBySession.removeValue(forKey: sessionId)
        renderStateBySession.removeValue(forKey: sessionId)
        resizeNotesBySession.removeValue(forKey: sessionId)
        TerminalInputQueue.shared.cancel(sessionId: sessionId)
        guard let existing else { return }
        disconnectTerminal(
            sessionId,
            existing.token.subscriptionId,
            existing.token.subscriptionGeneration
        )
    }

    private func createSession(sessionId: String) -> ManagedTerminalSession {
        Logger.terminal.info("Creating terminal session for \(sessionId)")
        let lastResize = ResizeDeliveryState()
        let token = TerminalConnectionToken()
        let runtimeIncarnationId = runtimeIncarnationsBySession[sessionId] ?? ""
        let resizeAuthority = TerminalResizeAuthorityCell(
            resizeAuthorityBySession[sessionId] == true
        )
        let inputFailure: @Sendable (TerminalInputFailure) -> Void = { [weak self, token] failure in
            guard token.isActive else { return }
            Task { @MainActor [weak self, token] in
                guard token.isActive else { return }
                self?.recordInputFailure(failure, sessionId: sessionId)
            }
        }
        let writer = TerminalWriteContext(
            sessionId: sessionId,
            runtimeHandle: runtimeHandle,
            token: token,
            runtimeIncarnationId: runtimeIncarnationId,
            onFailure: inputFailure
        )

        let renderer = TerminalRendererSession(
            write: { data in
                writer.send(data)
            },
            resize: { [weak self, token] viewport, surfaceGeneration in
                Task { @MainActor [weak self, token] in
                    guard token.isActive else { return }
                    self?.resizeViewport(viewport, sessionId: sessionId, surfaceGeneration: surfaceGeneration)
                }
            },
            onSurfaceAttached: { [weak self, token] _ in
                guard token.isActive else { return }
                self?.connectToRuntimeIfNeeded(sessionId: sessionId, token: token)
            }
        )

        renderer.setStyle(style())
        renderStateBySession[sessionId] = .connecting

        return ManagedTerminalSession(
            renderer: renderer,
            token: token,
            resizeAuthority: resizeAuthority,
            resizeDelivery: lastResize,
            runtimeIncarnationId: runtimeIncarnationId
        )
    }

    func noteCheckpointRestored(
        sessionId: String,
        token: TerminalConnectionToken,
        surfaceGeneration: UInt64
    ) {
        guard token.isActive,
              let managed = sessions[sessionId],
              managed.token === token
        else { return }
        managed.restoredSurfaceGeneration = surfaceGeneration
        renderStateBySession[sessionId] = .rendered
        if let viewport = managed.renderer.currentViewport {
            resizeViewport(viewport, sessionId: sessionId, surfaceGeneration: surfaceGeneration)
        }
    }

    private func resizeViewport(_ viewport: TerminalViewport, sessionId: String, surfaceGeneration: UInt64) {
        guard let managed = sessions[sessionId], managed.token.isActive,
              renderStateBySession[sessionId] == .rendered else { return }
        managed.pendingResize?.cancel()
        managed.pendingResize = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(80))
            guard let self, !Task.isCancelled, managed.token.isActive, managed.renderer.nativeSurfaceGeneration == surfaceGeneration else { return }
            TerminalResizeContext(
                sessionId: sessionId, commandSink: commandSink, lastResize: managed.resizeDelivery,
                resizeAuthority: managed.resizeAuthority, token: managed.token,
                runtimeIncarnationId: managed.runtimeIncarnationId, surfaceGeneration: surfaceGeneration
            ).deliver(viewport, claim: claimsSize(for: sessionId))
        }
    }

    public func claimsSize(for sessionId: String) -> Bool {
        claimsSizeByDefault || sizeClaims.contains(sessionId)
    }

    public func setClaimsSize(_ claims: Bool, for sessionId: String) {
        guard claims else {
            sizeClaims.remove(sessionId)
            return
        }
        sizeClaims.insert(sessionId)
        claimSize(for: sessionId)
    }

    private func claimSize(for sessionId: String) {
        guard let managed = sessions[sessionId], managed.token.isActive,
              renderStateBySession[sessionId] == .rendered,
              let viewport = managed.renderer.currentViewport
        else { return }
        managed.resizeDelivery.reset()
        resizeViewport(viewport, sessionId: sessionId, surfaceGeneration: managed.renderer.nativeSurfaceGeneration)
    }

    private func connectToRuntimeIfNeeded(
        sessionId: String,
        token: TerminalConnectionToken
    ) {
        guard token.isActive,
              let managed = sessions[sessionId],
              managed.token === token
        else { return }
        if managed.runtimeConnected {
            if managed.renderer.nativeSurfaceGeneration > managed.restoredSurfaceGeneration {
                refreshRendererSession(sessionId: sessionId, expectedToken: token)
            }
            return
        }
        managed.runtimeConnected = true
        connectToRuntime(sessionId: sessionId, token: token)
    }

    public func renderState(for sessionId: String) -> TerminalRenderState {
        renderStateBySession[sessionId] ?? .connecting
    }

    public func resizeNote(for sessionId: String) -> String? {
        resizeNotesBySession[sessionId]
    }

    public func retryTerminal(sessionId: String) {
        _ = replaceSession(sessionId: sessionId)
    }

    public func inputError(for sessionId: String) -> String? {
        inputErrorsBySession[sessionId]
    }

    public func clearInputError(for sessionId: String) {
        inputErrorsBySession.removeValue(forKey: sessionId)
    }

    private func recordInputFailure(_ failure: TerminalInputFailure, sessionId: String) {
        inputErrorsBySession[sessionId] = failure.userMessage
        Logger.terminal.error("Terminal input failure for \(sessionId): \(failure.userMessage)")
    }
}
