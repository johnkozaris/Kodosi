import KodosiBridge
import KodosiDesign
import KodosiTerminalSessions
import SwiftUI

struct SessionTileView: View {
    static let radius: CGFloat = 12

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.terminalTileVisible) private var tileVisible
    @Environment(\.terminalStageVisible) private var stageVisible
    let session: RuntimeSession
    let isFocused: Bool
    var roomEmbedded = false
    @State private var surfaceId = UUID()
    @State private var confirmingClose = false
    @State private var isClosing = false
    @State private var hovered = false
    @State private var born = true
    @State private var veil = 0.0
    @State private var joined: AvatarStack.Person?
    @State private var joinCount = 0

    private var isSelected: Bool {
        deps.workbench.selectedSessionId == session.id
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: Self.radius, style: .continuous)
    }

    var body: some View {
        let viewers = deps.viewers(of: session)
        VStack(spacing: 0) {
            SessionTileHeader(
                session: session, isFocused: isFocused, roomEmbedded: roomEmbedded,
                showsActions: hovered || isSelected, viewers: viewers, isClosing: isClosing,
            ) { confirmingClose = true }
                .background {
                    UnevenRoundedRectangle(topLeadingRadius: Self.radius, topTrailingRadius: Self.radius, style: .continuous)
                        .fill(theme.colors.raised.opacity(isSelected ? 0.75 : 0.45))
                }
                .contentShape(Rectangle()).onTapGesture(perform: select)
            terminal.frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.top, 3).padding([.horizontal, .bottom], 5)
        }
        .background {
            shape.fill(theme.colors.terminal)
                .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.14), radius: 10, x: 3, y: 6)
        }
        .overlay {
            let selected = isSelected && !roomEmbedded
            shape.strokeBorder(selected ? theme.colors.accent.mix(with: theme.colors.hairline, by: 0.55) : theme.colors.hairline.opacity(0.7),
                               lineWidth: selected ? 1 : 0.5)
                .allowsHitTesting(false)
        }
        .overlay {
            shape.fill(theme.colors.accent)
                .opacity(veil)
                .scaleEffect(born ? 1 : 0.03, anchor: UnitPoint(x: 0.04, y: 0.16))
                .allowsHitTesting(false)
        }
        .dosiTouch(deps.dosiTouches[session.id] ?? DosiTouch(), radius: Self.radius)
        .overlay(alignment: .topTrailing) {
            if let joined {
                JoinCard(person: joined)
                    .padding(.top, 44).padding(.trailing, 12)
                    .transition(.opacity.combined(with: .offset(y: -8)).combined(with: .scale(scale: 0.96, anchor: .topTrailing)))
                    .allowsHitTesting(false)
            }
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: isSelected)
        .onAppear {
            guard deps.freshTerminals.remove(session.id) != nil, !theme.motion.reduced else { return }
            born = false
            veil = 0.85
            withAnimation(theme.motion.soft) { born = true }
            withAnimation(theme.motion.soft?.delay(0.08)) { veil = 0 }
        }
        .onChange(of: tileVisible && stageVisible, initial: true) { _, _ in
            deps.reportVisible()
        }
        .onChange(of: viewers.map(\.id)) { old, new in
            guard let added = new.first(where: { !old.contains($0) }), let person = viewers.first(where: { $0.id == added }) else { return }
            withAnimation(theme.motion.spring) { joined = person }
            joinCount += 1
        }
        .task(id: joinCount) {
            guard joinCount > 0 else { return }
            try? await Task.sleep(for: .seconds(3.6))
            guard !Task.isCancelled else { return }
            withAnimation(theme.motion.soft) { joined = nil }
        }
        .onDisappear {
            deps.terminalFocus.release(surfaceId: surfaceId)
            deps.terminalFocus.registerRevoker(surfaceId: surfaceId, revoker: nil)
        }
        .confirmationDialog("Close \(session.name)?", isPresented: $confirmingClose, titleVisibility: .visible) {
            Button("Close", role: .destructive) { closeSession() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Its programs stop.")
        }
    }

    @ViewBuilder
    private var terminal: some View {
        if !session.isConnected {
            let away = session.status == .offline && session.connectionState != .blocked
            TerminalStateCard(
                symbol: session.connectionState == .blocked ? "lock" : "wifi.slash",
                title: session.statusLabel,
                message: away ? String(localized: "The terminal opens when its computer is back.") : session.message,
                working: session.connectionState == .connecting,
            ) {
                if session.connectionState == .offline, !away {
                    Button("Connect") { deps.activateSession(session.id) }
                        .buttonStyle(.kodosi(.primary))
                        .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).connect")
                } else if session.connectionState == .blocked {
                    Button("Open devices") { deps.workbench.showAccount() }
                }
            }
        } else {
            let manager = deps.terminalManager
            let managed = manager.session(for: session.id)
            let state = manager.renderState(for: session.id)
            ZStack {
                TerminalSurfaceWrapper(
                    renderer: managed.renderer, sessionId: session.id, sessionName: session.name, token: managed.token,
                    isFocused: isSelected && state == .rendered,
                    allowsInput: session.canControl && state == .rendered,
                    asksBeforeClipboardRead: !session.isOwner,
                    isSurfaceVisible: state == .rendered,
                ) {
                    select()
                } onFocusChange: { focused in
                    if focused, session.canControl {
                        deps.terminalFocus.acquire(
                            sessionId: session.id, runtimeIncarnationId: session.incarnationId,
                            subscriptionId: managed.token.subscriptionId,
                            subscriptionGeneration: managed.token.subscriptionGeneration, surfaceId: surfaceId,
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
                    TerminalStateCard(symbol: nil, title: String(localized: "Connecting"), message: nil, working: true) { EmptyView() }
                        .accessibilityLabel(Text("Connecting terminal"))
                        .background(theme.colors.terminal)
                case let .closed(message):
                    failure(message: message ?? String(localized: "The terminal closed."), manager: manager)
                case let .failed(message):
                    failure(message: message, manager: manager)
                }
            }
            if let message = manager.inputError(for: session.id) {
                HStack(spacing: 8) {
                    Circle().fill(theme.colors.caution).frame(width: 6, height: 6)
                    Text(message).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                    Spacer()
                    Button("Dismiss") { manager.clearInputError(for: session.id) }.buttonStyle(.kodosi(.ghost, size: .small))
                }
                .padding(.leading, 12).padding(.trailing, 4).frame(height: 34)
                .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: Radius.sm, style: .continuous))
                .padding(.top, 5)
            }
        }
    }

    private func failure(message: String, manager: TerminalSessionManager) -> some View {
        TerminalStateCard(symbol: "bolt.slash", title: String(localized: "This terminal is not available"), message: message, working: false) {
            Button("Try again") {
                deps.terminalFocus.releaseSession(sessionId: session.id)
                manager.retryTerminal(sessionId: session.id)
            }
            .buttonStyle(.kodosi(.primary))
            .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).terminal.retry")
        }
        .background(theme.colors.terminal)
    }

    private func select() {
        deps.workbench.selectSession(session.id)
        deps.reportVisible()
    }

    private func closeSession() {
        isClosing = true
        Task { @MainActor in
            await deps.close(session)
            isClosing = false
        }
    }
}

private struct JoinCard: View {
    @Environment(\.theme) private var theme
    let person: AvatarStack.Person

    var body: some View {
        HStack(spacing: 9) {
            PersonAvatar(name: person.name, key: person.id, size: 26)
            VStack(alignment: .leading, spacing: 1) {
                Text("\(person.name) joined").appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                Text("Full control").appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            }
        }
        .padding(.leading, 9).padding(.trailing, 14).frame(height: 44)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .floating)
        .accessibilityElement(children: .combine)
    }
}
