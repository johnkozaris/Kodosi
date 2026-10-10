import KodosiTerminal
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
                session: session, isFocused: isFocused, roomEmbedded: roomEmbedded, isSelected: isSelected,
                showsActions: hovered || isSelected, viewers: viewers, isClosing: isClosing
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
            shape.strokeBorder(isSelected && !roomEmbedded ? theme.colors.accent.opacity(0.75) : theme.colors.hairline.opacity(0.7),
                               lineWidth: isSelected && !roomEmbedded ? 1.5 : 0.5)
                .allowsHitTesting(false)
        }
        .overlay {
            shape.fill(theme.colors.accent)
                .opacity(veil)
                .scaleEffect(born ? 1 : 0.03, anchor: UnitPoint(x: 0.04, y: 0.16))
                .allowsHitTesting(false)
        }
        .overlay(alignment: .topTrailing) {
            if let joined {
                JoinCard(person: joined)
                    .keyframeAnimator(initialValue: 0.0, trigger: joinCount) { content, value in
                        content.opacity(value).offset(y: (1 - value) * -8).scaleEffect(0.96 + 0.04 * value, anchor: .topTrailing)
                    } keyframes: { _ in
                        KeyframeTrack {
                            SpringKeyframe(1.0, duration: 0.35, spring: .snappy)
                            LinearKeyframe(1.0, duration: 3.2)
                            CubicKeyframe(0.0, duration: 0.5)
                        }
                    }
                    .padding(.top, 44).padding(.trailing, 12)
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
            withAnimation(.interpolatingSpring(stiffness: 210, damping: 28)) { born = true }
            withAnimation(.easeOut(duration: 0.42).delay(0.08)) { veil = 0 }
        }
        .onChange(of: tileVisible && stageVisible, initial: true) { _, visible in
            if visible {
                deps.seen(session.id)
            }
        }
        .onChange(of: viewers.map(\.id)) { old, new in
            guard let added = new.first(where: { !old.contains($0) }), let person = viewers.first(where: { $0.id == added }) else { return }
            joined = person
            joinCount += 1
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
            TileState(
                symbol: session.connectionState == .blocked ? "lock" : "wifi.slash",
                title: session.statusLabel, message: session.message, working: session.connectionState == .connecting
            ) {
                if session.connectionState == .offline {
                    Button("Connect") { deps.activateSession(session.id) }
                        .buttonStyle(.kodosi(.primary))
                        .accessibilityIdentifier("\(AccessibilityIdentifier.stageSession(session.id)).connect")
                } else if session.connectionState == .blocked {
                    Button("Open devices") {
                        deps.workbench.settingsSection = .devices
                        deps.workbench.section = .settings
                    }
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
                    isSurfaceVisible: state == .rendered
                ) {
                    select()
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
                    TileState(symbol: nil, title: String(localized: "Connecting"), message: nil, working: true) { EmptyView() }
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
        TileState(symbol: "bolt.slash", title: String(localized: "This terminal is not available"), message: message, working: false) {
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
        deps.seen(session.id)
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

private struct TileState<Actions: View>: View {
    @Environment(\.theme) private var theme
    let symbol: String?
    let title: String
    let message: String?
    let working: Bool
    @ViewBuilder let actions: () -> Actions

    var body: some View {
        VStack(spacing: 14) {
            if let symbol {
                Image(systemName: symbol).font(.system(size: 20, weight: .medium)).foregroundStyle(theme.colors.inkMuted)
                    .frame(width: 48, height: 48).well(Radius.lg)
            } else {
                CursorBlock(width: 12, height: 22)
            }
            VStack(spacing: 5) {
                Text(title).appTextStyle(.headline).foregroundStyle(theme.colors.ink).shimmer(working)
                if let message {
                    Text(message).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .multilineTextAlignment(.center).frame(maxWidth: 320).textSelection(.enabled)
                }
            }
            actions()
        }
        .padding(20).frame(maxWidth: .infinity, maxHeight: .infinity)
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
