#if os(iOS)
    import GhosttyTerminal
    import SwiftUI
    import UIKit

    @MainActor
    public struct TerminalRendererView: View {
        @ObservedObject private var session: TerminalRendererSession
        private let sessionName: String
        private let accessibilityIdentifier: String
        private let shouldFocus: Bool
        private let allowsFocus: Bool
        private let allowsInput: Bool
        private let isSurfaceVisible: Bool
        private let onFocusChange: (Bool) -> Void

        public init(
            session: TerminalRendererSession,
            sessionName: String,
            accessibilityIdentifier: String,
            shouldFocus: Bool,
            allowsFocus: Bool,
            allowsInput: Bool,
            isSurfaceVisible: Bool,
            onFocusChange: @escaping (Bool) -> Void
        ) {
            _session = ObservedObject(wrappedValue: session)
            self.sessionName = sessionName
            self.accessibilityIdentifier = accessibilityIdentifier
            self.shouldFocus = shouldFocus
            self.allowsFocus = allowsFocus
            self.allowsInput = allowsInput
            self.isSurfaceVisible = isSurfaceVisible
            self.onFocusChange = onFocusChange
        }

        public var body: some View {
            TerminalRendererRepresentable(
                session: session,
                sessionName: sessionName,
                accessibilityIdentifier: accessibilityIdentifier,
                shouldFocus: shouldFocus,
                allowsFocus: allowsFocus,
                allowsInput: allowsInput,
                isSurfaceVisible: isSurfaceVisible,
                onFocusChange: onFocusChange
            )
        }
    }

    @MainActor
    final class TerminalRendererHostedView: UITerminalView {
        let delegateProxy = TerminalRendererDelegateProxy()
        var allowsFocus = true
        var requestedFocus = false

        @discardableResult
        override func becomeFirstResponder() -> Bool {
            guard allowsFocus else { return false }
            return super.becomeFirstResponder()
        }
    }

    @MainActor
    private struct TerminalRendererRepresentable: UIViewRepresentable {
        @ObservedObject var session: TerminalRendererSession
        let sessionName: String
        let accessibilityIdentifier: String
        let shouldFocus: Bool
        let allowsFocus: Bool
        let allowsInput: Bool
        let isSurfaceVisible: Bool
        let onFocusChange: (Bool) -> Void

        func makeUIView(context _: Context) -> TerminalRendererHostedView {
            let view = TerminalRendererHostedView(frame: .zero)
            view.delegateProxy.beginViewUpdate()
            configure(view)
            synchronizeFocus(view)
            view.delegateProxy.endViewUpdate()
            return view
        }

        func updateUIView(_ view: TerminalRendererHostedView, context _: Context) {
            view.delegateProxy.beginViewUpdate()
            configure(view)
            synchronizeFocus(view)
            view.delegateProxy.endViewUpdate()
        }

        static func dismantleUIView(_ view: TerminalRendererHostedView, coordinator _: ()) {
            view.onFocusChange = nil
            view.delegateProxy.target = nil
            view.delegateProxy.viewportDidResize = nil
            if view.isFirstResponder {
                view.resignFirstResponder()
            }
            view.delegate = nil
            view.tearDownSurface()
        }

        private func configure(_ view: TerminalRendererHostedView) {
            session.setAllowsInput(allowsInput)
            session.inputView = view
            view.allowsFocus = allowsFocus
            view.setSurfaceVisible(isSurfaceVisible)
            view.delegateProxy.target = session.viewState
            view.delegateProxy.viewportDidResize = nil
            if view.delegate !== view.delegateProxy {
                view.delegate = view.delegateProxy
            }
            if view.controller !== session.viewState.controller {
                view.controller = session.viewState.controller
            }
            view.configuration = session.viewState.configuration
            view.onFocusChange = { focused in
                onFocusChange(focused)
            }
            view.isAccessibilityElement = isSurfaceVisible
            view.accessibilityElementsHidden = !isSurfaceVisible
            view.accessibilityLabel = "Terminal for \(sessionName)"
            view.accessibilityHint = allowsInput
                ? "Interactive terminal showing the visible output."
                : "Read-only terminal showing the visible output."
            view.accessibilityIdentifier = accessibilityIdentifier
            if !allowsFocus, view.isFirstResponder {
                view.resignFirstResponder()
            }
        }

        private func synchronizeFocus(_ view: TerminalRendererHostedView) {
            let focusChanged = view.requestedFocus != shouldFocus
            view.requestedFocus = shouldFocus
            guard focusChanged, shouldFocus else { return }
            Task { @MainActor [weak view] in
                await Task.yield()
                guard let view, view.requestedFocus, view.window != nil, !view.isFirstResponder else { return }
                view.becomeFirstResponder()
            }
        }
    }
#endif
