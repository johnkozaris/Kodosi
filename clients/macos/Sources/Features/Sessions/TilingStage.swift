import SwiftUI

struct TilingStage: View {
    @Environment(\.theme) private var theme
    @Environment(AppDependencies.self) private var deps
    @Bindable var workbench: WorkbenchState

    private var presentedSessionIds: [String] {
        workbench.stagedSessionIds.filter { deps.session($0) != nil }
    }

    var body: some View {
        if presentedSessionIds.isEmpty {
            StageOverview().transition(.opacity)
        } else {
            TerminalTilesLayout {
                ForEach(presentedSessionIds, id: \.self) { id in
                    if let session = deps.session(id) {
                        let visible = workbench.focusedSessionId == nil || workbench.focusedSessionId == id
                        SessionTileView(session: session, isFocused: workbench.focusedSessionId == id)
                            .disabled(!visible)
                            .layoutValue(key: TerminalTileVisibility.self, value: visible)
                            .opacity(visible ? 1 : 0).allowsHitTesting(visible).accessibilityHidden(!visible)
                            .environment(\.terminalTileVisible, visible)
                            .terminalScope()
                    }
                }
            }
            .padding(6)
        }
    }
}

struct StageOverview: View {
    @Environment(\.theme) private var theme
    @Environment(AppDependencies.self) private var deps

    private struct Place: Identifiable {
        let id: String
        let label: String
        let sessions: [RuntimeSession]
    }

    private var places: [Place] {
        let grouped = Dictionary(grouping: deps.sessions) { session in
            session.kind == .local ? "" : session.hostDeviceId ?? session.ownerUserId ?? session.id
        }
        return grouped.map { key, sessions in
            let first = sessions[0]
            let label = first.kind == .local ? String(localized: "This Mac")
                : first.isOwner ? first.hostLabel : "\(first.ownerName ?? String(localized: "Shared")) · \(first.hostLabel)"
            return Place(id: key, label: label, sessions: sessions)
        }.sorted { ($0.id.isEmpty ? "" : $0.label) < ($1.id.isEmpty ? "" : $1.label) }
    }

    var body: some View {
        if deps.sessions.isEmpty {
            first
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 30) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Your terminals").appTextStyle(.large).foregroundStyle(theme.colors.ink)
                        Text(verbatim: "\(Counted.terminals(deps.sessions.count)) · \(Counted.computers(places.count))")
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }.arrive()
                    ForEach(Array(places.enumerated()), id: \.element.id) { index, place in
                        VStack(alignment: .leading, spacing: 12) {
                            HStack(spacing: 7) {
                                DeviceGlyph(label: place.label).font(.system(size: 12, weight: .medium))
                                Text(place.label).appTextStyle(.subhead)
                            }.foregroundStyle(theme.colors.inkMuted)
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 230, maximum: 320), spacing: 12)], alignment: .leading, spacing: 12) {
                                ForEach(place.sessions) { session in TerminalCard(session: session) }
                                if place.id.isEmpty {
                                    NewTerminalSlot(compact: true)
                                }
                            }
                        }.arrive(delay: 0.04 * Double(index + 1))
                    }
                }
                .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 36)
                .frame(maxWidth: 1040, alignment: .leading).frame(maxWidth: .infinity)
            }
        }
    }

    private var first: some View {
        VStack(spacing: 26) {
            NewTerminalSlot(compact: false).arrive()
            VStack(spacing: 6) {
                Text("Start with a terminal").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Text("It runs on this Mac. Share it when you want company.")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }.arrive(delay: 0.06)
            HStack(spacing: 8) {
                Button {
                    if let directory = pickWorkingDirectory(initialDirectory: deps.selectedLocalDirectory) {
                        deps.newSession(directory: directory)
                    }
                } label: { Label("Open a folder…", systemImage: "folder") }
                Button { deps.workbench.showsHistory = true } label: { Label("Resume a conversation", systemImage: "clock.arrow.circlepath") }
            }.buttonStyle(.kodosi(.ghost)).arrive(delay: 0.12)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct NewTerminalSlot: View {
    @Environment(\.theme) private var theme
    @Environment(AppDependencies.self) private var deps
    let compact: Bool

    var body: some View {
        Button { deps.newSession() } label: {
            DashedSlot(radius: compact ? Radius.lg : Radius.xl) {
                if compact {
                    HStack(spacing: 10) {
                        prompt(size: 15)
                        Text("New terminal").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 16).frame(height: 68)
                } else {
                    VStack(alignment: .leading) {
                        HStack(spacing: 6) {
                            ForEach(0 ..< 3, id: \.self) { _ in Circle().fill(theme.colors.inkFaint.opacity(0.35)).frame(width: 7, height: 7) }
                        }
                        Spacer()
                        prompt(size: 26)
                        Spacer()
                    }
                    .padding(18).frame(width: 400, height: 220, alignment: .leading)
                }
            }
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .accessibilityLabel(Text("New terminal")).accessibilityIdentifier("stage.newSession")
    }

    private func prompt(size: CGFloat) -> some View {
        HStack(spacing: size * 0.4) {
            Image(systemName: "chevron.right").font(.system(size: size * 0.8, weight: .heavy)).foregroundStyle(theme.colors.inkFaint)
            CursorBlock(width: size * 0.55, height: size)
        }
    }
}

struct TerminalCard: View {
    @Environment(\.theme) private var theme
    @Environment(AppDependencies.self) private var deps
    @State private var hovered = false
    let session: RuntimeSession

    var body: some View {
        Button { deps.activateSession(session.id) } label: {
            HStack(spacing: 12) {
                AgentMark(kind: session.agent, size: 34, activity: session.mark(rested: session.isConnected ? .awake : .asleep))
                VStack(alignment: .leading, spacing: 3) {
                    Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    SessionActivityText(session: session, fallback: session.folderName ?? session.agent.label)
                        .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer(minLength: 0)
                if let sign = deps.sign(of: session) {
                    StatusSign(form: sign, size: 13).transition(AnyTransition.pop)
                } else {
                    AvatarStack(people: deps.viewers(of: session), size: 18, limit: 2, ring: theme.colors.raised)
                }
            }
            .padding(.horizontal, 14).frame(height: 68)
            .raised(Radius.lg, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
            .offset(y: hovered ? -1 : 0)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .onHover { hovered = $0 }
        .animation(theme.motion.spring, value: hovered)
        .animation(theme.motion.snappy, value: deps.sign(of: session))
        .accessibilityIdentifier("overview.\(AccessibilityIdentifier.session(session.id))")
    }
}
