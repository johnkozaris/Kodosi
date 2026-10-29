import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomConversation: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let roomId: String
    @State private var position = ScrollPosition(edge: .bottom)
    @State private var touching = false

    private var rooms: RoomStore {
        model.rooms
    }

    private var targets: [RoomMention] {
        RoomMentions.targets(
            members: rooms.details[roomId]?.members ?? [],
            sessions: model.terminals.sessions.filter { $0.missionId == roomId },
            userId: rooms.userId,
        )
    }

    var body: some View {
        let state = rooms.view(roomId)
        let targets = targets
        Group {
            if let snapshot = rooms.snapshots[roomId] {
                transcript(snapshot, state: state, targets: targets)
                    .safeAreaInset(edge: .bottom) { RoomComposer(roomId: roomId, targets: targets) }
            } else if let failure = state.readFailure, !state.loading {
                RoomRecovery(message: failure) { Task { await rooms.read(roomId) } }
            } else {
                RoomSkeleton()
            }
        }
        .onAppear {
            state.watchingConversation = true
            if state.followsLatest {
                rooms.markRead(roomId)
            } else if let reading = state.readingMessage {
                position.scrollTo(id: reading, anchor: .top)
            }
        }
        .onDisappear {
            state.watchingConversation = false
            state.readingMessage = state.followsLatest ? nil : position.viewID(type: String.self)
        }
    }

    private func transcript(_ snapshot: RoomSnapshot, state: RoomViewState, targets: [RoomMention]) -> some View {
        let messages = snapshot.messages
        let unread = rooms.unread[roomId] ?? 0
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if snapshot.hasOlder, let first = messages.first {
                    earlier(before: first, state: state)
                } else if snapshot.historyTrimmed, !messages.isEmpty {
                    Text("Kodosi removed the oldest messages to make space for new ones.")
                        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                        .multilineTextAlignment(.center).frame(maxWidth: .infinity).padding(.bottom, 8)
                }
                ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                    RoomMessageRow(
                        roomId: roomId,
                        message: message,
                        previous: index > 0 ? messages[index - 1] : nil,
                        next: index + 1 < messages.count ? messages[index + 1] : nil,
                        targets: targets,
                    )
                    .transition(AnyTransition.rise)
                }
            }
            .scrollTargetLayout()
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 8)
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
            .animation(theme.motion.spring, value: messages.last?.id)
        }
        .scrollPosition($position)
        .defaultScrollAnchor(.bottom, for: .initialOffset)
        .scrollDismissesKeyboard(.interactively)
        .onScrollPhaseChange { _, phase in
            touching = phase == .interacting || phase == .decelerating
        }
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentOffset.y + geometry.containerSize.height - geometry.contentInsets.bottom >= geometry.contentSize.height - 40
        } action: { _, bottom in
            if bottom {
                state.followsLatest = true
                rooms.markRead(roomId)
            } else if touching {
                state.followsLatest = false
            }
        }
        .onScrollGeometryChange(for: [CGFloat].self) { geometry in
            [geometry.contentSize.height, geometry.containerSize.height]
        } action: { old, new in
            if state.followsLatest, old != new {
                position.scrollTo(edge: .bottom)
            }
        }
        .overlay {
            if messages.isEmpty {
                welcome(state: state)
            }
        }
        .overlay(alignment: .bottom) {
            if !state.followsLatest {
                latest(unread: unread, state: state)
            }
        }
        .animation(theme.motion.spring, value: state.followsLatest)
        .animation(theme.motion.snappy, value: unread)
    }

    private func welcome(state: RoomViewState) -> some View {
        var steps: [RoomWelcome.Step] = []
        if rooms.isOwner(of: roomId) {
            steps.append(RoomWelcome.Step(
                title: "Invite people", symbol: "person.badge.plus",
                done: (rooms.details[roomId]?.members.count ?? 0) > 1,
            ) { state.showsInfo = true })
        }
        steps.append(RoomWelcome.Step(
            title: "Add a terminal", symbol: "apple.terminal",
            done: model.terminals.sessions.contains { $0.missionId == roomId },
        ) { show(.terminals, in: state) })
        steps.append(RoomWelcome.Step(
            title: "Put up a task", symbol: "checklist",
            done: !(rooms.snapshots[roomId]?.tasks.isEmpty ?? true),
        ) { show(.tasks, in: state) })
        return RoomWelcome(name: rooms.mission(roomId)?.name ?? "", key: roomId, steps: steps)
    }

    private func show(_ section: RoomSection, in state: RoomViewState) {
        state.section = section
        state.shownSection = section
    }

    private func earlier(before first: RoomMessage, state: RoomViewState) -> some View {
        Button {
            Task {
                await rooms.read(roomId, before: first.sequence)
                position.scrollTo(id: first.id, anchor: .top)
            }
        } label: {
            Label(state.loading ? "Loading…" : "Earlier messages", systemImage: "arrow.up")
        }
        .buttonStyle(.kodosi(.ghost, size: .small))
        .frame(maxWidth: .infinity)
        .padding(.bottom, 8)
        .disabled(state.loading)
        .accessibilityIdentifier("rooms.messages.earlier")
    }

    private func latest(unread: Int, state: RoomViewState) -> some View {
        Button {
            state.followsLatest = true
            withAnimation(theme.motion.soft) { position.scrollTo(edge: .bottom) }
        } label: {
            Label(unread > 0 ? String(localized: "\(unread) new") : String(localized: "Latest"), systemImage: "arrow.down")
                .contentTransition(.numericText())
        }
        .buttonStyle(.kodosi(unread > 0 ? .primary : .secondary, size: .small))
        .padding(.bottom, 10)
        .transition(.move(edge: .bottom).combined(with: .opacity))
        .accessibilityIdentifier("rooms.messages.latest")
    }
}
