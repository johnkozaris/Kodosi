import KodosiTerminal
import SwiftUI

struct SessionTileView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let isFocused: Bool
    var roomEmbedded = false
    @State private var surfaceId = UUID()
    @State private var confirmingClose = false
    @State private var isClosing = false

    private var isSelected: Bool {
        deps.workbench.selectedSessionId == session.id
    }

    private var title: String {
        session.headerTitle
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 9) {
                SessionProgramIcon(program: session.program)
                if roomEmbedded, let room = session.missionId {
                    Menu {
                        ForEach(deps.sessions.filter { $0.missionId == room }) { candidate in
                            Button { deps.activateSession(candidate.id, inRoom: room) } label: {
                                if candidate.id == session.id {
                                    Label(candidate.name, systemImage: "checkmark")
                                } else {
                                    Text(candidate.name)
                                }
                            }.accessibilityIdentifier("room.terminal.\(AccessibilityIdentifier.token(candidate.id))")
                        }
                    } label: {
                        Text(title).appTextStyle(.headingItem).lineLimit(1)
                    }.menuStyle(.borderlessButton).frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityLabel(Text("Switch terminal")).accessibilityIdentifier("room.terminal.switcher")
                } else {
                    Text(title).appTextStyle(.headingItem).lineLimit(1).help(title)
                }
                if session.kind == .remote {
                    Text(session.hostLabel).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground).lineLimit(1)
                }
                Spacer(minLength: 4)
                if session.kind == .remote, session.isConnected, session.status == .reconnecting {
                    Text("Reconnecting").appTextStyle(.caption).foregroundStyle(theme.colors.statusWaiting)
                        .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).reconnecting")
                }
                if session.kind == .remote, session.isConnected, let notice = session.message {
                    Text(notice).appTextStyle(.caption).foregroundStyle(theme.colors.statusWaiting).lineLimit(1)
                        .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).notice")
                }
                HStack(spacing: 2) {
                    if !(session.connectedUsers ?? []).filter({ $0 != deps.userId }).isEmpty {
                        Image(systemName: "person.fill").foregroundStyle(theme.colors.primary)
                            .help("Someone else is connected. Open sharing for details.")
                            .accessibilityLabel(Text("Someone else is connected"))
                    }
                    if session.kind == .local, session.isOwner {
                        SessionIconButton(
                            title: "Share terminal", symbol: "person.badge.plus",
                            identifier: "\(AccessibilityIdentifier.stageSession(session.id)).share"
                        ) {
                            deps.workbench.sharingSessionId = session.id
                        }.popover(isPresented: Binding(get: { deps.workbench.sharingSessionId == session.id }, set: {
                            if !$0 {
                                deps.workbench.sharingSessionId = nil
                            }
                        })) {
                            SessionSharingPopover(session: session)
                        }
                    }
                    SessionIconButton(
                        title: "Session details", symbol: "slider.horizontal.3",
                        identifier: "\(AccessibilityIdentifier.stageSession(session.id)).details"
                    ) {
                        deps.workbench.detailsSessionId = session.id
                    }
                    SessionIconButton(title: "Minimize", symbol: "minus", identifier: "\(AccessibilityIdentifier.stageSession(session.id)).minimize") {
                        deps.dismissSession(session.id)
                    }
                    SessionIconButton(title: isFocused ? "Return to grid" : "Maximize",
                                      symbol: isFocused ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right",
                                      identifier: "\(AccessibilityIdentifier.stageSession(session.id)).focus")
                    {
                        deps.workbench.toggleFocus(session.id)
                    }.opacity(roomEmbedded ? 0 : 1).frame(width: roomEmbedded ? 0 : nil).disabled(roomEmbedded).accessibilityHidden(roomEmbedded)
                    SessionIconButton(
                        title: "Close terminal", symbol: "xmark",
                        identifier: "\(AccessibilityIdentifier.stageSession(session.id)).close", destructive: true
                    ) {
                        confirmingClose = true
                    }.disabled(!session.canControl || isClosing)
                }
            }
            .foregroundStyle(isSelected ? theme.colors.primary : theme.colors.foreground)
            .padding(.horizontal, 10).padding(.vertical, 7)
            .background(theme.colors.surfacePanel)
            .contentShape(Rectangle()).onTapGesture { deps.workbench.selectSession(session.id) }
            terminal.frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(theme.colors.surfaceTerminal)
        .onDisappear {
            deps.terminalFocus.release(surfaceId: surfaceId)
            deps.terminalFocus.registerRevoker(surfaceId: surfaceId, revoker: nil)
        }
        .confirmationDialog("Close \(session.name)?", isPresented: $confirmingClose, titleVisibility: .visible) {
            Button("Close", role: .destructive) { closeSession() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Running programs will stop.")
        }
    }

    @ViewBuilder
    private var terminal: some View {
        if !session.isConnected {
            VStack(spacing: 12) {
                Text(session.statusLabel).appTextStyle(.headingItem)
                if let message = session.message {
                    Text(message).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground).multilineTextAlignment(.center)
                }
                if session.connectionState == .offline {
                    Button("Connect") { deps.activateSession(session.id) }
                        .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).connect")
                        .buttonStyle(SolidSecondaryButtonStyle())
                } else {
                    Button("Open Devices") {
                        deps.workbench.settingsSection = .devices
                        deps.workbench.section = .settings
                    }
                    .buttonStyle(SolidSecondaryButtonStyle())
                }
            }
            .padding(20).frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            let manager = deps.terminalManager
            let managed = manager.session(for: session.id)
            let state = manager.renderState(for: session.id)
            ZStack {
                TerminalSurfaceWrapper(
                    renderer: managed.renderer, sessionId: session.id, sessionName: session.name, token: managed.token,
                    isFocused: isSelected && state == .rendered,
                    allowsInput: session.canControl && state == .rendered,
                    isSurfaceVisible: state == .rendered
                ) {
                    deps.workbench.selectSession(session.id)
                } onFocusChange: { focused in
                    if focused, session.canControl {
                        deps.terminalFocus.acquire(
                            sessionId: session.id, runtimeIncarnationId: session.incarnationId,
                            subscriptionId: managed.token.subscriptionId,
                            subscriptionGeneration: managed.token.subscriptionGeneration, surfaceId: surfaceId
                        )
                    } else {
                        deps.terminalFocus.release(surfaceId: surfaceId)
                    }
                } registerFocusRevoker: { revoker in
                    deps.terminalFocus.registerRevoker(surfaceId: surfaceId, revoker: revoker)
                } onShareShortcut: {
                    if session.kind == .local, session.isOwner {
                        deps.workbench.sharingSessionId = session.id
                    } else {
                        deps.workbench.detailsSessionId = session.id
                    }
                }
                .id(managed.token.subscriptionId)
                .accessibilityHidden(state != .rendered)
                switch state {
                case .rendered: EmptyView()
                case .connecting:
                    ProgressView().accessibilityLabel(Text("Connecting terminal"))
                        .frame(maxWidth: .infinity, maxHeight: .infinity).background(theme.colors.surfaceTerminal)
                case let .closed(message):
                    failure(message: message ?? String(localized: "The terminal closed."), manager: manager)
                case let .failed(message):
                    failure(message: message, manager: manager)
                }
            }
            if let message = manager.inputError(for: session.id) {
                HStack {
                    Text(message).appTextStyle(.caption).foregroundStyle(theme.colors.destructive)
                    Spacer()
                    Button("Dismiss") { manager.clearInputError(for: session.id) }.buttonStyle(.plain)
                }.padding(8).background(theme.colors.surfacePanel)
            }
        }
    }

    private func failure(message: String, manager: TerminalSessionManager) -> some View {
        VStack(spacing: 12) {
            Text("Terminal unavailable").appTextStyle(.headingItem)
            Text(message).appTextStyle(.caption).multilineTextAlignment(.center)
            Button("Retry") {
                deps.terminalFocus.releaseSession(sessionId: session.id)
                manager.retryTerminal(sessionId: session.id)
            }
            .buttonStyle(SolidSecondaryButtonStyle())
            .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).terminal.retry")
        }
        .padding(20).frame(maxWidth: .infinity, maxHeight: .infinity).background(theme.colors.surfaceTerminal)
    }

    private func closeSession() {
        isClosing = true
        Task { @MainActor in
            defer { isClosing = false }
            do {
                try await deps.mutateSession("session.close", session: session)
            } catch {
                deps.errorMessage = error.localizedDescription
            }
        }
    }
}
