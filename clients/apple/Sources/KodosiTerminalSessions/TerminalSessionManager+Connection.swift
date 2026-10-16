import Foundation
import KodosiBridge
import KodosiKit
import KodosiTerminal
import os

extension TerminalSessionManager {
    func connectToRuntime(sessionId: String, token: TerminalConnectionToken) {
        guard let managed = sessions[sessionId], managed.token === token else { return }
        let renderer = managed.renderer
        let resizeDelivery = managed.resizeDelivery
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
            onCheckpoint: checkpointHandler(link, renderer: renderer),
            onConnectionResult: connectionResultHandler(link, resizeDelivery: resizeDelivery)
        ))
    }

    func handleControlFrame(
        _ data: Data,
        sessionId: String,
        token: TerminalConnectionToken,
        resizeDelivery: ResizeDeliveryState? = nil
    ) {
        TerminalControlFrames.handle(
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
                    """, bundle: .module))
                },
                onFocusResult: { [weak self, token] requestId, incarnationId, accepted in
                    guard token.isActive, self?.sessions[sessionId]?.token === token else { return }
                    self?.onFocusResult(sessionId, incarnationId, requestId, accepted)
                },
                onResizeApplied: { [weak self, token] identity in
                    guard resizeDelivery?.apply(identity) == true else { return }
                    guard self?.sessions[sessionId]?.token === token else { return }
                    self?.resizeNotesBySession.removeValue(forKey: sessionId)
                },
                onResizeRejected: { [weak self, token] identity, reason in
                    guard resizeDelivery?.reject(identity) == true else { return }
                    guard let reason, self?.sessions[sessionId]?.token === token else { return }
                    self?.resizeNotesBySession[sessionId] = reason
                },
                onBell: { [weak self, token] in
                    guard token.isActive, self?.sessions[sessionId]?.token === token else { return }
                    self?.beep()
                }
            )
        )
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
                    localized: "This terminal fell behind. Try again.", bundle: .module
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
                self?.fail(link, message: String(localized: "This terminal fell behind. Try again.", bundle: .module))
            }
        }
    }

    func checkpointHandler(
        _ link: RuntimeLink, renderer: TerminalRendererSession
    ) -> TerminalSubscriptionRegistry.CheckpointHandler {
        { [weak self, weak renderer] snapshot, console in
            guard link.token.isActive, let renderer,
                  renderer.restoreCheckpointSynchronously(snapshot, console: console)
            else {
                return Int32(KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED)
            }
            link.awaitingResizeCheckpoint.withLock { $0 = false }
            Task { @MainActor [weak self, weak renderer] in
                guard link.token.isActive, let renderer, self?.sessions[link.sessionId]?.token === link.token else { return }
                self?.noteCheckpointRestored(
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
                        message ?? String(localized: "The terminal could not connect.", bundle: .module)
                    )
                }
            }
        }
    }
}
