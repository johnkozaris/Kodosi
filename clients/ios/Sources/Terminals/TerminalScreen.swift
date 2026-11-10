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
        TerminalScreenContent(store: model.terminals, lock: model.lock, sessionId: sessionId)
    }
}

private struct TerminalLiveKey: Equatable {
    let connected: Bool
    let canControl: Bool
    let incarnation: String?
    let rendered: Bool
    let replacement: String?
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
    @State private var canvasOpen = false
    @State private var addingFile = false
    @State private var keyboard = TerminalKeyboard(hardware: GCKeyboard.coalesced != nil)
    @FocusState private var composing: Bool

    init(store: TerminalStore, lock: AppLock, sessionId: String) {
        _controller = State(initialValue: TerminalController(sessionId: sessionId, store: store, lock: lock))
    }

    private var compact: Bool {
        verticalSizeClass == .compact
    }

    private var keyboardAllowed: Bool {
        controller.allowsKeyboard && !canvasOpen
    }

    private var input: TerminalInput {
        keyboard.input(composing: composing)
    }

    private var inputActions: TerminalInputActions {
        TerminalInputActions(
            type: {
                keyboard.type()
                composing = false
            },
            compose: {
                keyboard.compose()
            },
            hide: {
                keyboard.hide()
                composing = false
                UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
            },
        )
    }

    private var liveKey: TerminalLiveKey {
        let session = controller.session
        return TerminalLiveKey(
            connected: session?.isConnected == true,
            canControl: session?.canControl == true,
            incarnation: session?.incarnationId,
            rendered: controller.renderState == .rendered,
            replacement: controller.replacement?.id,
        )
    }

    var body: some View {
        let session = controller.session
        VStack(spacing: 0) {
            stage(session)
                .padding(.horizontal, compact ? 6 : 10)
                .padding(.top, input.showsHeader(compact: compact) ? 6 : 2)
                .padding(.bottom, session == nil ? 10 : 0)
            if let session {
                TerminalControls(
                    controller: controller, session: session, input: input, compact: compact, composing: $composing,
                    actions: inputActions,
                    menu: input.showsHeader(compact: compact)
                        ? nil
                        : TerminalMenu(session: session, prompt: $prompt, showsSharing: $showsSharing, capped: true,
                                       interrupt: controller.interrupt)
                        {
                            openBrowser(of: session)
                        } addFile: {
                            addingFile = true
                        },
                )
            }
        }
        .background(theme.colors.surface)
        .toolbar(input.showsHeader(compact: compact) ? .visible : .hidden, for: .navigationBar)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                if let session {
                    TerminalTitle(session: session, sign: model.terminals.sign(of: session), rename: session.isOwner ? { prompt = .rename(session) } : nil)
                }
            }
            .sharedBackgroundVisibility(.hidden)
            if let session {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    let viewers = model.viewers(of: session)
                    if !viewers.isEmpty {
                        AvatarStack(people: viewers, size: 22, limit: 3)
                            .accessibilityElement(children: .combine)
                            .accessibilityLabel(Text("Here now: \(viewers.map(\.name).joined(separator: ", "))"))
                    }
                    if session.canvas != nil {
                        CanvasKey(session: session, canvas: model.terminals.canvas) {
                            model.terminals.canvas.open(session)
                            canvasOpen = true
                        }
                    }
                    TerminalMenu(session: session, prompt: $prompt, showsSharing: $showsSharing, interrupt: controller.interrupt) {
                        openBrowser(of: session)
                    } addFile: {
                        addingFile = true
                    }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .toolbar(.hidden, for: .tabBar)
        .modifier(CanvasFilePicker(isPresented: $addingFile, session: session) { canvasOpen = true })
        .onAppear {
            controller.appear(active: scenePhase == .active)
            openShownCanvas()
            ScreenAwake.terminalShown()
        }
        .onDisappear {
            controller.disappear()
            ScreenAwake.terminalGone()
        }
        .onChange(of: scenePhase) { _, phase in
            controller.setActive(phase == .active)
            openShownCanvas()
        }
        .onChange(of: model.lock.locked) { _, locked in
            controller.refresh()
            openShownCanvas()
            if !locked, !canvasOpen, keyboard.input(composing: false) == .composing {
                composing = true
            }
        }
        .onChange(of: session?.canvas?.open) { _, _ in openShownCanvas() }
        .onChange(of: liveKey) { _, _ in controller.refresh() }
        .onChange(of: controller.sessionId) { _, _ in prompt = nil }
        .onChange(of: composing) { _, focused in
            if !focused {
                keyboard.composerClosed(locked: model.lock.locked)
            }
        }
        .onChange(of: session == nil || session?.hidden == true) { _, gone in
            if gone, controller.ending {
                dismiss()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidConnect)) { _ in keyboard.hardwareChanged(true) }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidDisconnect)) { _ in
            keyboard.hardwareChanged(GCKeyboard.coalesced != nil)
        }
        .terminalPrompts($prompt) { controller.noteEnding() }
        .sheet(isPresented: $showsSharing) {
            TerminalSharingSheet(sessionId: controller.sessionId)
        }
        .fullScreenCover(isPresented: $canvasOpen) {
            CanvasScreen(sessionId: controller.sessionId)
        }
        .sensoryFeedback(.selection, trigger: controller.presses)
        .sensoryFeedback(.impact(flexibility: .solid), trigger: controller.sent)
        .sensoryFeedback(.impact(weight: .light), trigger: controller.bells)
        .sensoryFeedback(.warning, trigger: session?.needsUser == true) { old, new in !old && new }
    }

    /// Opens the canvas at what the terminal's agent showed, once for each show.
    private func openShownCanvas() {
        if controller.openShownCanvas() {
            canvasOpen = true
        }
    }

    private func openBrowser(of session: RuntimeSession) {
        Task {
            do {
                try await model.terminals.canvas.openBrowser(of: session)
            } catch { model.notices.show(error) }
        }
        canvasOpen = true
    }

    private func stage(_ session: RuntimeSession?) -> some View {
        stageContent(session)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .terminalTile()
            .overlay(alignment: .top) {
                if let note = controller.sizeNote, session?.isConnected == true {
                    TerminalSizeNote(note: note, resize: controller.resizeHere)
                        .padding(8)
                        .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if let picture = controller.picture, let computer = session?.otherComputer {
                    PictureNotice(computer: computer, fraction: picture.fraction, arrived: picture.arrived)
                        .padding(10)
                        .transition(.opacity.combined(with: .offset(y: 8)))
                        .allowsHitTesting(false)
                }
            }
            .terminalScope()
            .animation(theme.motion.soft, value: liveKey)
            .animation(theme.motion.soft, value: controller.sizeNote)
            .animation(theme.motion.spring, value: controller.picture == nil)
    }

    @ViewBuilder
    private func stageContent(_ session: RuntimeSession?) -> some View {
        if let session {
            if session.isConnected, let surface = controller.surface {
                TerminalRowsFrame(renderer: surface.renderer) {
                    TerminalRendererView(
                        session: surface.renderer,
                        sessionName: session.name,
                        accessibilityIdentifier: "terminal.surface",
                        shouldFocus: keyboard.focusesTerminal(composing: composing) && controller.canType && keyboardAllowed,
                        allowsFocus: keyboardAllowed,
                        allowsInput: controller.canType,
                        asksBeforeClipboardRead: !session.isOwner,
                        otherComputer: session.otherComputer,
                        isSurfaceVisible: controller.presence.watching && controller.renderState == .rendered,
                        onFocusChange: { focused in
                            keyboard.terminalFocused(focused, allowed: keyboardAllowed)
                            if focused {
                                controller.focusTerminal()
                            }
                        },
                    )
                }
                .onDisappear { keyboard.terminalGone() }
                .padding(5)
                .overlay {
                    TerminalRenderOverlay(state: controller.renderState, retry: controller.retry)
                        .clipShape(RoundedRectangle(cornerRadius: Radius.tile, style: .continuous))
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
