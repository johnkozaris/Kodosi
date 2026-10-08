import AppKit
import GhosttyTerminal
import SwiftUI

public typealias TerminalFocusRevoker = @MainActor () -> Void

@MainActor
public struct TerminalRendererView: View {
    @ObservedObject private var session: TerminalRendererSession
    private let sessionId: String
    private let sessionName: String
    private let accessibilityIdentifier: String
    private let shouldFocus: Bool
    private let allowsFocus: Bool
    private let allowsInput: Bool
    private let isSurfaceVisible: Bool
    private let onFocus: () -> Void
    private let onFocusChange: (Bool) -> Void
    private let registerFocusRevoker: (TerminalFocusRevoker?) -> Void
    private let onShareShortcut: () -> Void

    public init(
        session: TerminalRendererSession,
        sessionId: String,
        sessionName: String,
        accessibilityIdentifier: String,
        shouldFocus: Bool,
        allowsFocus: Bool,
        allowsInput: Bool,
        isSurfaceVisible: Bool,
        onFocus: @escaping () -> Void,
        onFocusChange: @escaping (Bool) -> Void,
        registerFocusRevoker: @escaping (TerminalFocusRevoker?) -> Void = { _ in },
        onShareShortcut: @escaping () -> Void
    ) {
        _session = ObservedObject(wrappedValue: session)
        self.sessionId = sessionId
        self.sessionName = sessionName
        self.accessibilityIdentifier = accessibilityIdentifier
        self.shouldFocus = shouldFocus
        self.allowsFocus = allowsFocus
        self.allowsInput = allowsInput
        self.isSurfaceVisible = isSurfaceVisible
        self.onFocus = onFocus
        self.onFocusChange = onFocusChange
        self.registerFocusRevoker = registerFocusRevoker
        self.onShareShortcut = onShareShortcut
    }

    public var body: some View {
        TerminalRendererRepresentable(
            session: session,
            sessionId: sessionId,
            sessionName: sessionName,
            accessibilityIdentifier: accessibilityIdentifier,
            shouldFocus: shouldFocus,
            allowsFocus: allowsFocus,
            allowsInput: allowsInput,
            isSurfaceVisible: isSurfaceVisible,
            onFocus: onFocus,
            onFocusChange: onFocusChange,
            registerFocusRevoker: registerFocusRevoker,
            onShareShortcut: onShareShortcut
        )
    }
}

@MainActor
final class TerminalRendererHostedView: TerminalView {
    var shareShortcut: (() -> Void)?
    var registeredFocusRevoker: ((TerminalFocusRevoker?) -> Void)?
    let delegateProxy = TerminalRendererDelegateProxy()
    var requestedFocus = false
    var allowsFocus = true
    var allowsInput = true
    var viewportText: (() -> String?)?
    var viewportDidPotentiallyChange: (() -> Void)?
    weak var accessibilityUpdateSession: TerminalRendererSession?
    var accessibilityUpdateToken: UUID?
    private var accessibilityViewportText = ""
    private var isReadOnlySelectionDrag = false
    private var runtimeRejectedFocus = false

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard requestedFocus,
              canSynchronizeRequestedFocus,
              allowsFocus,
              let window,
              window.firstResponder !== self
        else { return }
        window.makeFirstResponder(self)
    }

    override func becomeFirstResponder() -> Bool {
        guard allowsFocus else { return false }
        runtimeRejectedFocus = false
        return super.becomeFirstResponder()
    }

    func revokeNativeFocus() {
        runtimeRejectedFocus = true
        if window?.firstResponder === self {
            window?.makeFirstResponder(nil)
        }
    }

    var canSynchronizeRequestedFocus: Bool {
        !runtimeRejectedFocus
    }

    override func accessibilityValue() -> Any? {
        refreshAccessibilityViewport(postNotification: false)
        return accessibilityViewportText
    }

    override func accessibilityVisibleCharacterRange() -> NSRange {
        refreshAccessibilityViewport(postNotification: false)
        return NSRange(location: 0, length: accessibilityViewportText.utf16.count)
    }

    override func accessibilityNumberOfCharacters() -> Int {
        refreshAccessibilityViewport(postNotification: false)
        return accessibilityViewportText.utf16.count
    }

    override func accessibilityString(for range: NSRange) -> String? {
        refreshAccessibilityViewport(postNotification: false)
        let text = accessibilityViewportText as NSString
        guard range.location != NSNotFound,
              range.location <= text.length,
              range.length <= text.length - range.location
        else { return nil }
        return text.substring(with: range)
    }

    @discardableResult
    func refreshAccessibilityViewport(
        postNotification: Bool = true
    ) -> Bool {
        let text = viewportText?() ?? ""
        guard text != accessibilityViewportText else { return false }
        accessibilityViewportText = text
        if postNotification {
            NSAccessibility.post(element: self, notification: .valueChanged)
        }
        return true
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let modifiers = event.modifierFlags
            .intersection(.deviceIndependentFlagsMask)
        if modifiers == [.command, .shift],
           event.charactersIgnoringModifiers?.lowercased() == "s"
        {
            shareShortcut?()
            return true
        }
        if !allowsInput,
           modifiers == [.command],
           event.charactersIgnoringModifiers?.lowercased() == "c"
        {
            copy(nil)
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func tryToPerform(_ action: Selector, with object: Any?) -> Bool {
        if !allowsInput, action == NSSelectorFromString("paste:") {
            return true
        }
        return super.tryToPerform(action, with: object)
    }

    override func keyDown(with event: NSEvent) {
        guard allowsInput else { return }
        super.keyDown(with: event)
        viewportDidPotentiallyChange?()
    }

    override func keyUp(with event: NSEvent) {
        guard allowsInput else { return }
        super.keyUp(with: event)
    }

    override func flagsChanged(with event: NSEvent) {
        guard allowsInput else { return }
        super.flagsChanged(with: event)
    }

    override func doCommand(by selector: Selector) {
        guard allowsInput else { return }
        super.doCommand(by: selector)
        viewportDidPotentiallyChange?()
    }

    override func insertText(_ string: Any, replacementRange: NSRange) {
        guard allowsInput else { return }
        super.insertText(string, replacementRange: replacementRange)
        viewportDidPotentiallyChange?()
    }

    override func setMarkedText(
        _ string: Any,
        selectedRange: NSRange,
        replacementRange: NSRange
    ) {
        guard allowsInput else { return }
        super.setMarkedText(
            string,
            selectedRange: selectedRange,
            replacementRange: replacementRange
        )
        viewportDidPotentiallyChange?()
    }

    override func mouseDown(with event: NSEvent) {
        guard allowsInput else {
            isReadOnlySelectionDrag = true
            super.mouseDown(with: selectionEvent(from: event))
            return
        }
        super.mouseDown(with: event)
    }

    override func mouseUp(with event: NSEvent) {
        guard allowsInput else {
            defer { isReadOnlySelectionDrag = false }
            guard isReadOnlySelectionDrag else { return }
            super.mouseUp(with: selectionEvent(from: event))
            return
        }
        super.mouseUp(with: event)
    }

    override func rightMouseDown(with event: NSEvent) {
        guard allowsInput else {
            if let menu = selectionContextMenu() {
                NSMenu.popUpContextMenu(menu, with: event, for: self)
            }
            return
        }
        super.rightMouseDown(with: event)
    }

    override func rightMouseUp(with event: NSEvent) {
        guard allowsInput else { return }
        super.rightMouseUp(with: event)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        guard allowsInput else { return selectionContextMenu() }
        return super.menu(for: event)
    }

    override func otherMouseDown(with event: NSEvent) {
        guard allowsInput else { return }
        super.otherMouseDown(with: event)
    }

    override func otherMouseUp(with event: NSEvent) {
        guard allowsInput else { return }
        super.otherMouseUp(with: event)
    }

    override func mouseMoved(with event: NSEvent) {
        guard allowsInput else { return }
        super.mouseMoved(with: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard allowsInput else {
            guard isReadOnlySelectionDrag else { return }

            allowsInput = true
            defer { allowsInput = false }
            super.mouseDragged(with: selectionEvent(from: event))
            return
        }
        super.mouseDragged(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        super.scrollWheel(with: event)
        viewportDidPotentiallyChange?()
    }

    override func rightMouseDragged(with event: NSEvent) {
        guard allowsInput else { return }
        super.rightMouseDragged(with: event)
    }

    override func otherMouseDragged(with event: NSEvent) {
        guard allowsInput else { return }
        super.otherMouseDragged(with: event)
    }

    private func selectionEvent(from event: NSEvent) -> NSEvent {
        guard let translated = NSEvent.mouseEvent(
            with: event.type,
            location: event.locationInWindow,
            modifierFlags: event.modifierFlags.union(.shift),
            timestamp: event.timestamp,
            windowNumber: event.windowNumber,
            context: nil,
            eventNumber: event.eventNumber,
            clickCount: event.clickCount,
            pressure: event.pressure
        ) else {
            return event
        }
        return translated
    }
}

@MainActor
private struct TerminalRendererRepresentable: NSViewRepresentable {
    @ObservedObject var session: TerminalRendererSession
    let sessionId: String
    let sessionName: String
    let accessibilityIdentifier: String
    let shouldFocus: Bool
    let allowsFocus: Bool
    let allowsInput: Bool
    let isSurfaceVisible: Bool
    let onFocus: () -> Void
    let onFocusChange: (Bool) -> Void
    let registerFocusRevoker: (TerminalFocusRevoker?) -> Void
    let onShareShortcut: () -> Void

    func makeNSView(context _: Context) -> TerminalRendererHostedView {
        let view = TerminalRendererHostedView(frame: .zero)
        view.delegateProxy.beginViewUpdate()
        configure(view)
        synchronizeFocus(view)
        view.delegateProxy.endViewUpdate()
        return view
    }

    func updateNSView(_ view: TerminalRendererHostedView, context _: Context) {
        view.delegateProxy.beginViewUpdate()
        configure(view)
        synchronizeFocus(view)
        view.delegateProxy.endViewUpdate()
    }

    static func dismantleNSView(
        _ view: TerminalRendererHostedView,
        coordinator _: ()
    ) {
        view.onFocusChange = nil
        view.shareShortcut = nil
        view.viewportText = nil
        view.viewportDidPotentiallyChange = nil
        cancelAccessibilityUpdates(for: view)
        view.registeredFocusRevoker?(nil)
        view.registeredFocusRevoker = nil
        view.delegateProxy.target = nil
        view.delegateProxy.viewportDidResize = nil
        if view.window?.firstResponder === view {
            view.window?.makeFirstResponder(nil)
        }
        view.delegate = nil
        view.tearDownSurface()
    }

    private func configure(_ view: TerminalRendererHostedView) {
        session.setAllowsInput(allowsInput)
        view.allowsFocus = allowsFocus
        view.allowsInput = allowsInput
        view.setSurfaceVisible(isSurfaceVisible)
        view.delegateProxy.target = session.viewState
        view.delegateProxy.viewportDidResize = {
            session.signalAccessibilityViewportChange()
        }
        if view.delegate !== view.delegateProxy {
            view.delegate = view.delegateProxy
        }
        if view.controller !== session.viewState.controller {
            view.controller = session.viewState.controller
        }
        view.configuration = session.viewState.configuration
        view.onFocusChange = { focused in
            onFocusChange(focused)
            if focused {
                onFocus()
            }
        }
        view.shareShortcut = onShareShortcut
        view.viewportText = { session.readViewportText() }
        view.viewportDidPotentiallyChange = {
            session.signalAccessibilityViewportChange()
        }
        if view.accessibilityUpdateSession !== session {
            Self.cancelAccessibilityUpdates(for: view)
            view.accessibilityUpdateSession = session
            view.accessibilityUpdateToken = session.registerAccessibilityUpdateHandler { [weak view] in
                view?.refreshAccessibilityViewport() == true
            }
        }
        view.refreshAccessibilityViewport()
        view.registeredFocusRevoker = registerFocusRevoker
        registerFocusRevoker { [weak view] in
            view?.revokeNativeFocus()
        }
        view.setAccessibilityElement(isSurfaceVisible)
        view.setAccessibilityHidden(!isSurfaceVisible)
        view.setAccessibilityRole(.textArea)
        view.setAccessibilityLabel("Terminal for \(sessionName)")
        view.setAccessibilityHelp(
            allowsInput
                ? "Interactive terminal showing the visible output."
                : "Read-only terminal showing the visible output. You can review, select, and copy text."
        )
        view.setAccessibilityIdentifier(accessibilityIdentifier)
        if !allowsFocus, view.window?.firstResponder === view {
            view.window?.makeFirstResponder(nil)
        }
    }

    private static func cancelAccessibilityUpdates(
        for view: TerminalRendererHostedView
    ) {
        if let session = view.accessibilityUpdateSession,
           let token = view.accessibilityUpdateToken
        {
            session.unregisterAccessibilityUpdateHandler(token)
        }
        view.accessibilityUpdateSession = nil
        view.accessibilityUpdateToken = nil
    }

    private func synchronizeFocus(_ view: TerminalRendererHostedView) {
        let focusChanged = view.requestedFocus != shouldFocus
        view.requestedFocus = shouldFocus
        guard focusChanged else { return }
        Task { @MainActor [weak view] in
            await Task.yield()
            guard let view,
                  view.requestedFocus == shouldFocus,
                  let window = view.window
            else {
                return
            }
            if shouldFocus, view.canSynchronizeRequestedFocus {
                if window.firstResponder !== view {
                    window.makeFirstResponder(view)
                }
            } else if window.firstResponder === view {
                window.makeFirstResponder(nil)
            }
        }
    }
}
