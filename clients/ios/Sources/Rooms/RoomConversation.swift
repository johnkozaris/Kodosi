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
        .showsRoomPart(.conversation, of: state)
        .onAppear {
            if !state.followsLatest, let reading = state.readingMessage {
                position.scrollTo(id: reading, anchor: .top)
            }
        }
        .onDisappear {
            state.readingMessage = state.followsLatest ? nil : position.viewID(type: String.self)
        }
    }

    private func transcript(_ snapshot: RoomSnapshot, state: RoomViewState, targets: [RoomMention]) -> some View {
        let messages = snapshot.messages
        let unread = state.unread
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if snapshot.hasOlder, let first = messages.first {
                    RoomEarlierKey(loading: state.loading, identifier: "rooms.messages.earlier") {
                        Task {
                            await rooms.read(roomId, before: first.sequence)
                            position.scrollTo(id: first.id, anchor: .top)
                        }
                    }
                    .padding(.bottom, 8)
                } else if snapshot.historyTrimmed, !messages.isEmpty {
                    RoomTrimmedNote().padding(.bottom, 8)
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
            geometry.contentOffset.y + geometry.contentInsets.top + geometry.containerSize.height >= geometry.contentSize.height - 40
        } action: { _, bottom in
            if bottom {
                state.followsLatest = true
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
                welcome()
            }
        }
        .overlay(alignment: .bottom) {
            if !state.followsLatest {
                RoomLatestKey(unread: unread, identifier: "rooms.messages.latest") {
                    state.followsLatest = true
                    withAnimation(theme.motion.soft) { position.scrollTo(edge: .bottom) }
                }
                .padding(.bottom, 10)
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(theme.motion.spring, value: state.followsLatest)
        .animation(theme.motion.snappy, value: unread)
    }

    private func welcome() -> some View {
        let screen = model.roomScreen(roomId)
        var steps: [RoomWelcome.Step] = []
        if rooms.isOwner(of: roomId) {
            steps.append(RoomWelcome.Step(
                title: "Invite people", symbol: "person.badge.plus", tint: TileTint.blue,
                done: (rooms.details[roomId]?.members.count ?? 0) > 1,
            ) { screen.showsInfo = true })
        }
        steps.append(RoomWelcome.Step(
            title: "Add a terminal", symbol: "apple.terminal", tint: TileTint.graphite,
            done: model.terminals.sessions.contains { $0.missionId == roomId },
        ) { show(.terminals, in: screen) })
        steps.append(RoomWelcome.Step(
            title: "Put up a task", symbol: "checklist", tint: TileTint.orange,
            done: !(rooms.snapshots[roomId]?.tasks.isEmpty ?? true),
        ) { show(.tasks, in: screen) })
        return RoomWelcome(name: rooms.mission(roomId)?.name ?? "", key: roomId, steps: steps)
    }

    private func show(_ section: RoomSection, in screen: RoomScreenState) {
        screen.section = section
        screen.shownSection = section
    }
}
