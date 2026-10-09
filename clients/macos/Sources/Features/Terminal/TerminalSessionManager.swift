import CoreFoundation
import Foundation
import KodosiKit
import KodosiTerminal
import Observation
import os
import SwiftUI

struct TerminalWriteContext: Sendable {
    let sessionId: String
    let runtimeHandle: RuntimeHandle
    let token: TerminalConnectionToken
    let runtimeIncarnationId: String
    let onFailure: TerminalInputQueue.FailureHandler
}

@MainActor
final class ManagedTerminalSession {
    let renderer: TerminalRendererSession
    let token: TerminalConnectionToken
    let resizeAuthority: TerminalResizeAuthorityCell
    let resizeDelivery: ResizeDeliveryState
    let runtimeIncarnationId: String
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

enum TerminalRenderState: Equatable {
    case connecting
    case rendered
    case closed(String?)
    case failed(String)
}

@MainActor
@Observable

final class TerminalSessionManager {
    private var inputErrorsBySession: [String: String] = [:]
    private var renderStateBySession: [String: TerminalRenderState] = [:]
    private var runtimeIncarnationsBySession: [String: String] = [:]
    private var resizeAuthorityBySession: [String: Bool] = [:]

    private let runtimeHandle: RuntimeHandle
    private let commandSink: CommandSink
    private let beep: @MainActor () -> Void
    private let onFocusResult: @MainActor (String, String, String, Bool) -> Void
    private let disconnectTerminal: @MainActor (
        _ sessionId: String,
        _ subscriptionId: String,
        _ subscriptionGeneration: UInt64
    ) -> Void
    let settings: DesktopSettings
    private var sessions: [String: ManagedTerminalSession] = [:]

    init(
        runtimeHandle: RuntimeHandle,
        commandSink: CommandSink,
        settings: DesktopSettings,
        disconnectTerminal: (@MainActor (
            _ sessionId: String,
            _ subscriptionId: String,
            _ subscriptionGeneration: UInt64
        ) -> Void)? = nil,
        beep: @escaping @MainActor () -> Void = { NSSound.beep() },
        onFocusResult: @escaping @MainActor (String, String, String, Bool) -> Void = { _, _, _, _ in }
    ) {
        self.runtimeHandle = runtimeHandle
        self.commandSink = commandSink
        self.settings = settings
        self.disconnectTerminal = disconnectTerminal ?? { sessionId, subscriptionId, subscriptionGeneration in
            runtimeHandle.disconnectTerminal(
                sessionId: sessionId,
                subscriptionId: subscriptionId,
                subscriptionGeneration: subscriptionGeneration
            )
        }
        self.beep = beep
        self.onFocusResult = onFocusResult
    }

    func session(for sessionId: String) -> ManagedTerminalSession {
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

    func reconcileSessions(
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

    func releaseSession(for sessionId: String) {
        let existing = sessions.removeValue(forKey: sessionId)
        existing?.pendingResize?.cancel()
        existing?.controls.close()
        existing?.controlTask?.cancel()
        existing?.resizeDelivery.reset()
        existing?.token.deactivate()
        inputErrorsBySession.removeValue(forKey: sessionId)
        renderStateBySession.removeValue(forKey: sessionId)
        TerminalInputQueue.shared.cancel(sessionId: sessionId)
        guard let existing else { return }
        disconnectTerminal(
            sessionId,
            existing.token.subscriptionId,
            existing.token.subscriptionGeneration
        )
    }

    func applySettings(_ terminalSettings: DesktopSettings.TerminalSettings) {
        let fontSize = Float(terminalSettings.fontSize)
        for managed in sessions.values {
            managed.renderer.setStyle(kodosiTerminalStyle(
                settings: terminalSettings,
                fontSize: fontSize
            ))
        }
    }

    private func createSession(sessionId: String) -> ManagedTerminalSession {
        Logger.terminal.info("Creating terminal session for \(sessionId)")
        let rtHandle = runtimeHandle
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

        let renderer = TerminalRendererSession(
            write: { [token] data in
                Self.handleTerminalWrite(
                    data,
                    context: TerminalWriteContext(
                        sessionId: sessionId,
                        runtimeHandle: rtHandle,
                        token: token,
                        runtimeIncarnationId: runtimeIncarnationId,
                        onFailure: inputFailure
                    )
                )
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

        renderer.setStyle(kodosiTerminalStyle())
        renderStateBySession[sessionId] = .connecting

        return ManagedTerminalSession(
            renderer: renderer,
            token: token,
            resizeAuthority: resizeAuthority,
            resizeDelivery: lastResize,
            runtimeIncarnationId: runtimeIncarnationId
        )
    }

    private func noteSemanticCheckpointRestored(
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
            Self.handleTerminalResize(viewport, context: TerminalResizeContext(
                sessionId: sessionId, commandSink: commandSink, lastResize: managed.resizeDelivery,
                resizeAuthority: managed.resizeAuthority, token: managed.token,
                runtimeIncarnationId: managed.runtimeIncarnationId, surfaceGeneration: surfaceGeneration
            ), claim: true)
        }
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
        connectToRuntime(
            sessionId: sessionId,
            renderer: managed.renderer,
            token: token,
            resizeDelivery: managed.resizeDelivery
        )
    }

    private func connectToRuntime(
        sessionId: String,
        renderer: TerminalRendererSession,
        token: TerminalConnectionToken,
        resizeDelivery: ResizeDeliveryState
    ) {
        guard let managed = sessions[sessionId], managed.token === token else { return }
        let controls = managed.controls
        managed.controlTask = Task { @MainActor [weak self, token] in
            while let data = await controls.next() {
                guard !Task.isCancelled, token.isActive, self?.sessions[sessionId]?.token === token else { return }
                self?.handleControlFrame(data, sessionId: sessionId, token: token, resizeDelivery: resizeDelivery)
            }
        }
        let link = RuntimeLink(sessionId: sessionId, token: token, runtime: runtimeHandle)
        runtimeHandle.connectTerminal(sessionId: sessionId, connection: .init(
            subscriptionId: token.subscriptionId,
            subscriptionGeneration: token.subscriptionGeneration,
            onData: outputHandler(link, renderer: renderer),
            onControl: controlHandler(link, controls: controls),
            onSemanticCheckpoint: checkpointHandler(link, renderer: renderer),
            onConnectionResult: connectionResultHandler(link, resizeDelivery: resizeDelivery)
        ))
    }

    func renderState(for sessionId: String) -> TerminalRenderState {
        renderStateBySession[sessionId] ?? .connecting
    }

    func retryTerminal(sessionId: String) {
        _ = replaceSession(sessionId: sessionId)
    }

    func inputError(for sessionId: String) -> String? {
        inputErrorsBySession[sessionId]
    }

    func clearInputError(for sessionId: String) {
        inputErrorsBySession.removeValue(forKey: sessionId)
    }
}

extension TerminalSessionManager {
    func handleControlFrame(
        _ data: Data,
        sessionId: String,
        token: TerminalConnectionToken,
        resizeDelivery: ResizeDeliveryState? = nil
    ) {
        Self.handleControlFrame(
            data,
            sessionId: sessionId,
            handlers: .init(
                onClosed: { [weak self, token, resizeDelivery] reason in
                    resizeDelivery?.reset()
                    token.deactivate()
                    guard self?.sessions[sessionId]?.token === token else { return }
                    self?.renderStateBySession[sessionId] = .closed(reason)
                },
                onInvalidControl: { [weak self, token] in
                    guard token.isActive, self?.sessions[sessionId]?.token === token else { return }
                    self?.renderStateBySession[sessionId] = .failed(String(localized: """
                    The terminal received an invalid control frame. \
                    Retry the terminal to reconnect.
                    """))
                },
                onFocusResult: { [weak self, token] requestId, incarnationId, accepted in
                    guard token.isActive, self?.sessions[sessionId]?.token === token else { return }
                    self?.onFocusResult(sessionId, incarnationId, requestId, accepted)
                },
                onResizeApplied: { identity in
                    guard resizeDelivery?.apply(identity) == true else { return }
                },
                onResizeRejected: { identity in
                    guard resizeDelivery?.reject(identity) == true else { return }
                },
                onBell: { [weak self, token] in
                    guard token.isActive, self?.sessions[sessionId]?.token === token else { return }
                    self?.beep()
                }
            )
        )
    }

    private func recordInputFailure(_ failure: TerminalInputFailure, sessionId: String) {
        inputErrorsBySession[sessionId] = failure.userMessage
        Logger.terminal.error("Terminal input failure for \(sessionId): \(failure.userMessage)")
    }
}

private struct RuntimeLink: Sendable {
    let sessionId: String
    let token: TerminalConnectionToken
    let runtime: RuntimeHandle
    let awaitingResizeCheckpoint = OSAllocatedUnfairLock(initialState: false)
}

private extension TerminalSessionManager {
    func fail(_ link: RuntimeLink, message: String) {
        guard sessions[link.sessionId]?.token === link.token else { return }
        link.runtime.disconnectTerminal(sessionId: link.sessionId, subscriptionId: link.token.subscriptionId,
                                        subscriptionGeneration: link.token.subscriptionGeneration)
        renderStateBySession[link.sessionId] = .failed(message)
    }

    func outputHandler(_ link: RuntimeLink, renderer: TerminalRendererSession) -> @Sendable (Data) -> Void {
        { [weak self, weak renderer] data in
            guard link.token.isActive, let renderer else { return }
            guard !link.awaitingResizeCheckpoint.withLock({ $0 }) else { return }
            guard !renderer.receive(data), link.token.deactivate() else { return }
            Task { @MainActor [weak self] in
                self?.fail(link, message: String(
                    localized: "This terminal fell behind. Try again."
                ))
            }
        }
    }

    func controlHandler(_ link: RuntimeLink, controls: RuntimeEventInbox) -> @Sendable (Data) -> Void {
        { [weak self] data in
            guard link.token.isActive else { return }
            if let fields = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               fields["type"] as? String == "term.resize"
            {
                link.awaitingResizeCheckpoint.withLock { $0 = true }
                link.runtime.refreshTerminal(sessionId: link.sessionId, subscriptionId: link.token.subscriptionId,
                                             subscriptionGeneration: link.token.subscriptionGeneration)
                return
            }
            guard !controls.push(data), link.token.deactivate() else { return }
            controls.close(overflowed: true)
            Task { @MainActor [weak self] in
                self?.fail(link, message: String(localized: "This terminal fell behind. Try again."))
            }
        }
    }

    func checkpointHandler(
        _ link: RuntimeLink, renderer: TerminalRendererSession
    ) -> TerminalSubscriptionRegistry.SemanticCheckpointHandler {
        { [weak self, weak renderer] checkpoint in
            guard link.token.isActive, let renderer, renderer.restoreCheckpointSynchronously(checkpoint.bytes) else {
                return Int32(KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED)
            }
            link.awaitingResizeCheckpoint.withLock { $0 = false }
            Task { @MainActor [weak self, weak renderer] in
                guard link.token.isActive, let renderer, self?.sessions[link.sessionId]?.token === link.token else { return }
                self?.noteSemanticCheckpointRestored(
                    sessionId: link.sessionId, token: link.token, surfaceGeneration: renderer.nativeSurfaceGeneration
                )
            }
            return Int32(KODOSI_FFI_OK)
        }
    }

    func connectionResultHandler(
        _ link: RuntimeLink, resizeDelivery: ResizeDeliveryState
    ) -> TerminalSubscriptionRegistry.ConnectionResult {
        { [weak self] accepted, message in
            Task { @MainActor [weak self] in
                guard link.token.isActive else { return }
                if accepted {
                    if self?.renderStateBySession[link.sessionId] != .rendered {
                        self?.renderStateBySession[link.sessionId] = .rendered
                    }
                } else {
                    resizeDelivery.reset()
                    link.token.deactivate()
                    self?.renderStateBySession[link.sessionId] = .failed(
                        message ?? String(localized: "The terminal could not connect.")
                    )
                }
            }
        }
    }
}
