import KodosiBridge
import KodosiDesign
import SwiftUI

struct TilingStage: View {
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
        Dictionary(grouping: deps.sessions, by: SessionFolderGroup.computer).map { key, sessions in
            Place(id: key, label: SessionFolderGroup.host(of: sessions[0]) ?? String(localized: "This Mac"), sessions: sessions)
        }.sorted { ($0.id == "local" ? "" : $0.label) < ($1.id == "local" ? "" : $1.label) }
    }

    var body: some View {
        if deps.sessions.isEmpty {
            first
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 30) {
                    AgentLauncher()
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
                                ForEach(place.sessions) { session in
                                    TerminalCard(session: session, rested: session.isConnected ? .awake : .asleep) {
                                        deps.activateSession(session.id)
                                    } detail: {
                                        SessionActivityText(session: session, fallback: session.folderName ?? session.agent.label)
                                    } accessory: {
                                        if let sign = deps.sign(of: session) {
                                            StatusSign(form: sign, size: 13).transition(AnyTransition.pop)
                                        } else {
                                            AvatarStack(people: deps.viewers(of: session), size: 18, limit: 2, ring: theme.colors.raised)
                                        }
                                    }
                                    .animation(theme.motion.snappy, value: deps.sign(of: session))
                                    .accessibilityIdentifier("overview.\(AccessibilityIdentifier.session(session.id))")
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
        GeometryReader { proxy in
            AgentLauncher(welcome: true)
                .frame(maxWidth: .infinity)
                .padding(.top, max(40, proxy.size.height / 2 - 150))
        }
    }
}

struct TerminalCard<Detail: View, Accessory: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let session: RuntimeSession
    let rested: AgentMark.Activity
    let open: () -> Void
    @ViewBuilder let detail: Detail
    @ViewBuilder let accessory: Accessory

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                AgentMark(kind: session.agent, size: 34, activity: session.mark(rested: rested))
                VStack(alignment: .leading, spacing: 3) {
                    Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    detail.appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer(minLength: 0)
                accessory
            }
            .padding(.horizontal, 14).frame(height: 68)
            .raised(Radius.lg, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
            .offset(y: hovered ? -1 : 0)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .onHover { hovered = $0 }
        .animation(theme.motion.spring, value: hovered)
    }
}
