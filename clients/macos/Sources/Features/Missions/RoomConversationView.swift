import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomConversationView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    let detail: MissionDetail
    @Bindable var state: RoomViewState
    let workspace: RoomWorkspace
    @State private var copied = false

    private var messages: [RoomMessage] {
        deps.rooms.snapshots[roomId]?.messages ?? []
    }

    private var targets: [RoomMention] {
        RoomMentions.targets(members: detail.members, sessions: deps.roomSessions(roomId), userId: deps.account.userId)
    }

    var body: some View {
        let targets = targets
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text("Conversation").appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
                Spacer()
                Button(action: copyInvite) {
                    Label(copied ? String(localized: "Copied. Paste it to your agent") : String(localized: "Bring an agent"),
                          systemImage: copied ? "checkmark" : "sparkles")
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(.kodosi(copied ? .tinted : .ghost, size: .small))
                .help("Copy an invite that tells an agent how to join this room")
                .accessibilityLabel(Text("Copy agent instructions")).accessibilityIdentifier("room.agent.instructions")
                .onHover { inside in
                    if !inside {
                        copied = false
                    }
                }
            }
            .padding(.leading, 16).padding(.trailing, 8).frame(height: 46)
            if deps.rooms.snapshots[roomId] == nil {
                if let failure = state.readFailure, !state.loading {
                    RoomRecovery(message: failure) { Task { await deps.rooms.read(roomId) } }
                } else {
                    RoomSkeleton()
                }
            } else {
                transcript(targets)
                RoomComposer(roomId: roomId, state: state, targets: targets)
            }
        }
        .raised(Radius.xl, fill: theme.colors.sheet)
        .showsRoomPart(.conversation, of: state)
        .animation(theme.motion.snappy, value: copied)
        .environment(\.openURL, OpenURLAction { url in
            guard let id = RoomMentions.terminalId(in: url) else { return .systemAction }
            if deps.session(id) != nil {
                workspace.shownSection = 0
                workspace.selectedTerminal = id
                deps.activateSession(id, inRoom: roomId)
            }
            return .handled
        })
    }

    private func transcript(_ targets: [RoomMention]) -> some View {
        let messages = messages
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if deps.rooms.snapshots[roomId]?.hasOlder == true, let first = messages.first {
                        RoomEarlierKey(loading: state.loading, identifier: "room.messages.earlier") {
                            Task { await deps.rooms.read(roomId, before: first.sequence); proxy.scrollTo(first.id, anchor: .top) }
                        }
                        .padding(.bottom, 10)
                    } else if deps.rooms.snapshots[roomId]?.historyTrimmed == true, !messages.isEmpty {
                        RoomTrimmedNote().padding(.bottom, 10)
                    }
                    ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                        let previous = index > 0 ? messages[index - 1] : nil
                        RoomMessageRow(
                            roomId: roomId, message: message, previous: previous, targets: targets, state: state, workspace: workspace,
                        )
                        .id(message.id)
                    }
                    Color.clear.frame(height: 1).id("latest")
                }
                .scrollTargetLayout().padding(.horizontal, 14).padding(.vertical, 10)
            }
            .defaultScrollAnchor(.bottom)
            .scrollPosition(id: $state.readingMessage, anchor: .bottom)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 36
            } action: { _, latest in
                state.followsLatest = latest
            }
            .overlay {
                if messages.isEmpty {
                    RoomWelcome(name: detail.mission.name, key: roomId, steps: [
                        RoomWelcome.Step(title: "Invite people", symbol: "person.badge.plus", tint: TileTint.blue,
                                         done: detail.members.count > 1) { workspace.peopleVisible = true },
                        RoomWelcome.Step(title: "Add a terminal", symbol: "apple.terminal", tint: TileTint.graphite,
                                         done: !deps.roomSessions(roomId).isEmpty) { deps.openLauncher(roomId: roomId) },
                        RoomWelcome.Step(title: copied ? "Copied. Paste it to your agent" : "Bring an agent in", symbol: "sparkles",
                                         tint: TileTint.orange, done: copied, action: copyInvite),
                    ])
                }
            }
            .overlay(alignment: .bottom) {
                if !state.followsLatest {
                    RoomLatestKey(unread: state.unread, identifier: "room.messages.latest") {
                        withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                    }
                    .padding(10)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(theme.motion.spring, value: state.followsLatest)
            .animation(theme.motion.spring, value: messages.last?.id)
            .onChange(of: messages.last?.id) { _, _ in
                if state.followsLatest {
                    withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                }
            }
            .onChange(of: state.followsLatest) { _, follows in
                if follows {
                    withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                }
            }
        }
    }

    private func copyInvite() {
        let path = (Bundle.main.executablePath ?? "kodosi").replacingOccurrences(of: "'", with: "'\"'\"'")
        deps.copyText("""
        Read `'\(path)' skill`, then use \
        `'\(path)' --json room --room \(roomId) context`.
        Use this executable for the skill's kodosi commands. You can read and post updates, \
        create and pick up tasks, and share results for this work.
        """)
        copied = true
    }
}
