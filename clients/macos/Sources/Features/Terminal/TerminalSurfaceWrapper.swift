import AppKit
import KodosiTerminal
import SwiftUI

extension EnvironmentValues {
    @Entry var terminalInteractionEnabled: Bool = true
    @Entry var terminalStageVisible: Bool = true
    @Entry var terminalTileVisible: Bool = true
}

struct TerminalSurfaceWrapper: View {
    @ObservedObject var renderer: TerminalRendererSession
    let sessionId: String
    let sessionName: String
    let token: TerminalConnectionToken
    let isFocused: Bool
    let allowsInput: Bool
    let isSurfaceVisible: Bool
    let onFocus: () -> Void
    let onFocusChange: (Bool) -> Void
    let registerFocusRevoker: (TerminalFocusRevoker?) -> Void
    let onShareShortcut: () -> Void
    @Environment(\.theme) private var theme
    @Environment(\.terminalTileVisible) private var tileVisible
    @Environment(\.terminalStageVisible) private var stageVisible
    @Environment(\.terminalInteractionEnabled) private var terminalInteractionEnabled

    private var interactionAllowed: Bool {
        terminalInteractionEnabled && stageVisible && tileVisible
    }

    private var inputAllowed: Bool {
        interactionAllowed && allowsInput && isSurfaceVisible
    }

    var body: some View {
        TerminalRendererView(
            session: renderer,
            sessionId: sessionId,
            sessionName: sessionName,
            accessibilityIdentifier: "\(AccessibilityIdentifier.stageSession(sessionId)).terminal",
            shouldFocus: isFocused && interactionAllowed,
            allowsFocus: interactionAllowed && isSurfaceVisible,
            allowsInput: inputAllowed,
            isSurfaceVisible: isSurfaceVisible && stageVisible && tileVisible,
            onFocus: onFocus,
            onFocusChange: onFocusChange,
            registerFocusRevoker: registerFocusRevoker,
            onShareShortcut: onShareShortcut
        )
        .background(theme.colors.terminal)
        .onChange(of: inputAllowed, initial: true) { _, enabled in
            token.setInputAllowed(enabled)
            renderer.setAllowsInput(enabled)
            if !enabled {
                TerminalInputQueue.shared.cancel(sessionId: sessionId)
                onFocusChange(false)
            }
        }
        .onDisappear {
            token.setInputAllowed(false)
            TerminalInputQueue.shared.cancel(sessionId: sessionId)
        }
    }
}
