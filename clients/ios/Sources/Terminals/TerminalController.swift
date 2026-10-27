import Foundation
import KodosiBridge
import KodosiTerminal
import KodosiTerminalSessions
import Observation
import UIKit

struct TerminalPresence: Equatable {
    var visible = false
    var active = false
    var rendered = false
    var canControl = false

    var watching: Bool {
        visible && active
    }

    var focused: Bool {
        watching && rendered && canControl
    }
}

enum TerminalMessage {
    static func text(from draft: String) -> String? {
        var text = draft
        while let last = text.unicodeScalars.last, CharacterSet.newlines.contains(last) {
            text.unicodeScalars.removeLast()
        }
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return text
    }

    static func fits(_ text: String) -> Bool {
        text.utf8.count <= TerminalInputQueue.chunkBytes
    }
}

@Observable
final class TerminalController {
    private(set) var sessionId: String
    let surfaceId = UUID()
    var draft = ""
    private(set) var presence = TerminalPresence()
    private(set) var holdsFocus = false
    private(set) var focusDeclined = false
    private(set) var opening = false
    private(set) var openFailure: String?
    private(set) var inputNote: String?
    private(set) var ending = false
    private(set) var sent = 0
    private(set) var presses = 0

    @ObservationIgnored let store: TerminalStore
    @ObservationIgnored private var openTask: Task<Void, Never>?
    @ObservationIgnored private var listed: RuntimeSession?

    init(sessionId: String, store: TerminalStore) {
        self.sessionId = sessionId
        self.store = store
    }

    var session: RuntimeSession? {
        store.session(sessionId)
    }

    var replacement: RuntimeSession? {
        guard !ending, let listed else { return nil }
        return store.replacement(of: listed)
    }

    var surface: ManagedTerminalSession? {
        guard presence.visible, session?.isConnected == true else { return nil }
        return store.live.session(for: sessionId)
    }

    var renderState: TerminalRenderState {
        store.live.renderState(for: sessionId)
    }

    var sizeNote: String? {
        store.live.resizeNote(for: sessionId)
    }

    var inputError: String? {
        store.live.inputError(for: sessionId)
    }

    var canType: Bool {
        presence.focused
    }

    var allowsKeyboard: Bool {
        presence.visible && presence.rendered && presence.canControl
    }

    var bells: Int {
        store.bells[sessionId] ?? 0
    }

    func appear(active: Bool) {
        guard !presence.visible else { return }
        presence.visible = true
        presence.active = active
        store.focus.registerRevoker(surfaceId: surfaceId) { [weak self] in
            self?.focusRevoked()
        }
        refresh()
        open()
    }

    func disappear() {
        guard presence.visible else { return }
        presence.visible = false
        leave()
        store.focus.registerRevoker(surfaceId: surfaceId, revoker: nil)
    }

    func setActive(_ active: Bool) {
        guard presence.active != active else { return }
        presence.active = active
        refresh()
        guard active, presence.visible else { return }
        recover()
    }

    func refresh() {
        if let replacement {
            follow(replacement)
        } else if let session {
            listed = session
        }
        var current = presence
        current.rendered = renderState == .rendered
        current.canControl = session?.canControl == true
        if current != presence {
            presence = current
        }
        store.watch(sessionId, presence.watching)
        guard presence.visible, let surface else {
            releaseFocus()
            return
        }
        let focused = presence.focused
        surface.token.setInputAllowed(focused, by: surfaceId)
        guard focused, !focusDeclined, let session else {
            if !focused {
                TerminalInputQueue.shared.cancel(sessionId: sessionId)
            }
            releaseFocus()
            return
        }
        store.focus.acquire(
            sessionId: sessionId,
            runtimeIncarnationId: session.incarnationId,
            subscriptionId: surface.token.subscriptionId,
            subscriptionGeneration: surface.token.subscriptionGeneration,
            surfaceId: surfaceId,
        )
        holdsFocus = true
    }

    func focusTerminal() {
        focusDeclined = false
        refresh()
    }

    func open() {
        guard openTask == nil, presence.visible, let session, session.canOpen else { return }
        opening = true
        openFailure = nil
        openTask = Task { [weak self, store, sessionId] in
            do {
                try await store.open(sessionId)
                self?.finishOpen(sessionId, failure: nil)
            } catch is CancellationError {
                return
            } catch {
                self?.finishOpen(sessionId, failure: error.localizedDescription)
            }
        }
    }

    func retry() {
        store.focus.releaseSession(sessionId: sessionId)
        holdsFocus = false
        focusDeclined = false
        store.live.retryTerminal(sessionId: sessionId)
        refresh()
    }

    func send() -> Bool {
        guard let text = TerminalMessage.text(from: draft) else { return false }
        guard TerminalMessage.fits(text) else {
            inputNote = String(localized: "This message is too long to send at once.")
            return false
        }
        guard canType, let renderer = surface?.renderer, renderer.pasteText(text) else { return false }
        renderer.sendKeyPress(.keyboardReturnOrEnter)
        draft = ""
        inputNote = nil
        sent += 1
        return true
    }

    func press(_ key: TerminalKey) {
        guard canType, let renderer = surface?.renderer, renderer.sendKeyPress(key.usage, modifiers: key.modifiers) else { return }
        presses += 1
    }

    func dismissInputNotes() {
        inputNote = nil
        store.live.clearInputError(for: sessionId)
    }

    func interrupt() {
        guard let session else { return }
        store.interrupt(session)
    }

    func noteEnding() {
        ending = true
    }

    private func finishOpen(_ opened: String, failure: String?) {
        guard opened == sessionId else { return }
        openTask = nil
        opening = false
        openFailure = failure
    }

    private func follow(_ next: RuntimeSession) {
        leave()
        sessionId = next.id
        listed = next
        openFailure = nil
        focusDeclined = false
        open()
    }

    private func leave() {
        openTask?.cancel()
        openTask = nil
        opening = false
        releaseFocus()
        store.watch(sessionId, false)
        TerminalInputQueue.shared.cancel(sessionId: sessionId)
        store.live.releaseSession(for: sessionId)
        store.disconnect(sessionId)
    }

    private func recover() {
        guard let session else { return }
        if !session.isConnected || openFailure != nil {
            open()
        }
        switch renderState {
        case .failed, .closed:
            if session.isConnected {
                retry()
            }
        case .connecting, .rendered:
            break
        }
    }

    private func releaseFocus() {
        guard holdsFocus else { return }
        holdsFocus = false
        store.focus.release(surfaceId: surfaceId)
    }

    private func focusRevoked() {
        holdsFocus = false
        focusDeclined = true
    }
}
