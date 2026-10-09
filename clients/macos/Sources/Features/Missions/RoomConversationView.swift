import AppKit
import SwiftUI

struct RoomConversationView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    let detail: MissionDetail
    @Bindable var state: RoomViewState
    @State private var copied = false

    private var messages: [RoomMessage] {
        deps.rooms[roomId]?.messages ?? []
    }

    private var targets: [MentionTarget] {
        RoomMentions.targets(members: detail.members, sessions: deps.sessions.filter { $0.missionId == roomId }, userId: deps.userId)
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
            if deps.rooms[roomId] == nil {
                if let failure = state.failure, !state.loading {
                    RoomRecovery(message: failure) { Task { await deps.readRoom(roomId) } }
                } else {
                    RoomSkeleton()
                }
            } else {
                transcript(targets)
                RoomComposer(roomId: roomId, state: state, targets: targets)
            }
        }
        .raised(Radius.xl, fill: theme.colors.raised.mix(with: theme.colors.surface, by: 0.45))
        .animation(theme.motion.snappy, value: copied)
        .environment(\.openURL, OpenURLAction { url in
            guard url.scheme == "kodosi-mention" else { return .systemAction }
            let id = url.lastPathComponent
            if deps.session(id) != nil {
                state.canvas = 0
                state.selectedTerminal = id
                deps.activateSession(id, inRoom: roomId)
            }
            return .handled
        })
    }

    private func transcript(_ targets: [MentionTarget]) -> some View {
        let messages = messages
        return ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if deps.rooms[roomId]?.hasOlder == true, let first = messages.first {
                        Button {
                            Task { await deps.readRoom(roomId, before: first.sequence); proxy.scrollTo(first.id, anchor: .top) }
                        } label: { Label("Earlier messages", systemImage: "arrow.up") }
                            .buttonStyle(.kodosi(.ghost, size: .small)).frame(maxWidth: .infinity)
                            .accessibilityIdentifier("room.messages.earlier").disabled(state.loading).padding(.bottom, 10)
                    }
                    ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                        let previous = index > 0 ? messages[index - 1] : nil
                        RoomMessageRow(
                            roomId: roomId, message: message, previous: previous, targets: targets, state: state
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
                if latest {
                    state.unread = 0
                }
            }
            .overlay {
                if messages.isEmpty {
                    RoomWelcome(roomId: roomId, detail: detail, state: state, copyInvite: copyInvite, copied: copied)
                }
            }
            .overlay(alignment: .bottom) {
                if !state.followsLatest {
                    Button {
                        withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                    } label: {
                        Label(state.unread > 0 ? String(localized: "\(state.unread) new") : String(localized: "Latest"), systemImage: "arrow.down")
                    }
                    .buttonStyle(.kodosi(state.unread > 0 ? .primary : .secondary, size: .small)).padding(10)
                    .accessibilityIdentifier("room.messages.latest")
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
        }
    }

    private func copyInvite() {
        let path = (Bundle.main.executablePath ?? "kodosi").replacingOccurrences(of: "'", with: "'\"'\"'")
        NSPasteboard.general.clearContents()
        let instructions = """
        Read `'\(path)' room skill`, then use \
        `'\(path)' --json room --room \(roomId) context`.
        Use this executable for the skill's kodosi commands. You can read and post updates, \
        create and pick up tasks, and share results for this work.
        """
        NSPasteboard.general.setString(instructions, forType: .string)
        copied = true
    }
}

private struct RoomWelcome: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    let detail: MissionDetail
    @Bindable var state: RoomViewState
    let copyInvite: () -> Void
    let copied: Bool

    var body: some View {
        let hasTerminal = deps.sessions.contains { $0.missionId == roomId }
        VStack(spacing: 18) {
            RoomSigil(name: detail.mission.name, key: roomId, size: 52).arrive()
            VStack(spacing: 4) {
                Text("This is \(detail.mission.name)").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.center)
                Text("Three steps to get going").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }.arrive(delay: 0.04)
            VStack(spacing: 6) {
                step("Invite people", symbol: "person.badge.plus", tint: TileTint.blue, done: detail.members.count > 1) {
                    state.peopleVisible = true
                }
                step("Add a terminal", symbol: "apple.terminal", tint: TileTint.graphite, done: hasTerminal) {
                    deps.newRoomTerminal(roomId)
                }
                step(copied ? "Copied. Paste it to your agent" : "Bring an agent in", symbol: "sparkles", tint: TileTint.orange, done: copied,
                     action: copyInvite)
            }.arrive(delay: 0.08)
        }
        .padding(18).frame(maxWidth: 300)
    }

    private func step(_ title: LocalizedStringKey, symbol: String, tint: Color, done: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 11) {
                IconTile(symbol: symbol, tint: tint, size: 28)
                Text(title).appTextStyle(.body).foregroundStyle(done ? theme.colors.inkMuted : theme.colors.ink)
                Spacer(minLength: 4)
                Image(systemName: done ? "checkmark.circle.fill" : "chevron.right")
                    .font(.system(size: done ? 15 : 10, weight: .semibold))
                    .foregroundStyle(done ? theme.colors.ready : theme.colors.inkFaint)
                    .contentTransition(.symbolEffect(.replace))
            }
            .padding(.horizontal, 10).frame(height: 46)
            .raised(Radius.lg, fill: theme.colors.lifted)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
    }
}
