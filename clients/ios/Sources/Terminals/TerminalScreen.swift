import Combine
import GameController
import KodosiBridge
import KodosiDesign
import KodosiTerminal
import KodosiTerminalSessions
import SwiftUI

struct TerminalScreen: View {
    @Environment(AppModel.self) private var model
    let sessionId: String

    var body: some View {
        TerminalScreenContent(store: model.terminals, sessionId: sessionId)
    }
}

private struct TerminalLiveKey: Equatable {
    let connected: Bool
    let canControl: Bool
    let incarnation: String?
    let rendered: Bool
}

private struct TerminalScreenContent: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dismiss) private var dismiss
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @State private var controller: TerminalController
    @State private var prompt: TerminalPrompt?
    @State private var showsSharing = false
    @State private var hardwareKeyboard = GCKeyboard.coalesced != nil
    @FocusState private var composing: Bool

    init(store: TerminalStore, sessionId: String) {
        _controller = State(initialValue: TerminalController(sessionId: sessionId, store: store))
    }

    private var compact: Bool {
        verticalSizeClass == .compact
    }

    private var liveKey: TerminalLiveKey {
        let session = controller.session
        return TerminalLiveKey(
            connected: session?.isConnected == true,
            canControl: session?.canControl == true,
            incarnation: session?.incarnationId,
            rendered: controller.renderState == .rendered,
        )
    }

    var body: some View {
        let session = controller.session
        VStack(spacing: 0) {
            stage(session)
                .padding(.horizontal, compact ? 6 : 10)
                .padding(.top, 6)
                .padding(.bottom, session == nil ? 10 : 0)
            if let session {
                TerminalControls(controller: controller, session: session, compact: compact, composing: $composing)
            }
        }
        .background { GroundBackdrop() }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                if let session {
                    TerminalTitle(session: session)
                }
            }
            if let session {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    let viewers = model.viewers(of: session)
                    if !viewers.isEmpty {
                        AvatarStack(people: viewers, size: 24, limit: 3, ring: theme.colors.surface)
                            .accessibilityElement(children: .combine)
                            .accessibilityLabel(Text("Here now: \(viewers.map(\.name).joined(separator: ", "))"))
                    }
                    TerminalMenu(session: session, prompt: $prompt, showsSharing: $showsSharing, interrupt: controller.interrupt)
                }
            }
        }
        .toolbar(.hidden, for: .tabBar)
        .onAppear { controller.appear(active: scenePhase == .active) }
        .onDisappear { controller.disappear() }
        .onChange(of: scenePhase) { _, phase in controller.setActive(phase == .active) }
        .onChange(of: liveKey) { _, _ in controller.refresh() }
        .onChange(of: session == nil || session?.hidden == true) { _, gone in
            if gone, controller.ending {
                dismiss()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidConnect)) { _ in hardwareKeyboard = true }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidDisconnect)) { _ in
            hardwareKeyboard = GCKeyboard.coalesced != nil
        }
        .terminalPrompts($prompt) { controller.noteEnding() }
        .sheet(isPresented: $showsSharing) {
            TerminalSharingSheet(sessionId: controller.sessionId)
        }
        .sensoryFeedback(.selection, trigger: controller.presses)
        .sensoryFeedback(.impact(flexibility: .solid), trigger: controller.sent)
        .sensoryFeedback(.impact(weight: .light), trigger: controller.bells)
        .sensoryFeedback(.warning, trigger: session?.needsUser == true) { old, new in !old && new }
    }

    private func stage(_ session: RuntimeSession?) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        return stageContent(session)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background { shape.fill(theme.colors.terminal) }
            .overlay { shape.strokeBorder(theme.colors.hairline.opacity(0.7), lineWidth: 0.5).allowsHitTesting(false) }
            .overlay(alignment: .top) {
                if let note = controller.sizeNote, session?.isConnected == true {
                    TerminalSizeNote(note: note, resize: controller.focusTerminal)
                        .padding(8)
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .terminalScope()
            .animation(theme.motion.soft, value: liveKey)
            .animation(theme.motion.soft, value: controller.sizeNote)
    }

    @ViewBuilder
    private func stageContent(_ session: RuntimeSession?) -> some View {
        if let session {
            if session.isConnected, let surface = controller.surface {
                TerminalRendererView(
                    session: surface.renderer,
                    sessionName: session.name,
                    accessibilityIdentifier: "terminal.surface",
                    shouldFocus: hardwareKeyboard && !composing && controller.canType,
                    allowsFocus: controller.allowsKeyboard,
                    allowsInput: controller.canType,
                    asksBeforeClipboardRead: !session.isOwner,
                    isSurfaceVisible: controller.presence.watching && controller.renderState == .rendered,
                    onFocusChange: { focused in
                        if focused {
                            controller.focusTerminal()
                        }
                    },
                )
                .padding(5)
                .overlay {
                    TerminalRenderOverlay(state: controller.renderState, retry: controller.retry)
                        .clipShape(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
                }
                .id(surface.token.subscriptionId)
            } else {
                TerminalSessionState(session: session, opening: controller.opening, failure: controller.openFailure, connect: controller.open)
            }
        } else {
            TerminalEnded { dismiss() }
        }
    }
}
