import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: MissionDetail
    let state: RoomViewState
    @Bindable var workspace: RoomWorkspace
    @State private var dragStart: CGFloat?
    @State private var handleHovered = false
    @State private var dropping = false

    private var roomId: String {
        detail.mission.id
    }

    var body: some View {
        VStack(spacing: 0) {
            RoomHeader(detail: detail, state: state, workspace: workspace)
            GeometryReader { geometry in
                let narrow = geometry.size.width < 720
                let width = min(max(workspace.conversationWidth, 300), max(300, min(540, geometry.size.width - 380)))
                ZStack(alignment: .trailing) {
                    if !(narrow && workspace.conversationVisible) {
                        RoomContentView(roomId: roomId, state: state, workspace: workspace)
                            .overlay {
                                DropSlot(radius: Radius.xl, shown: dropping, label: String(localized: "Share with \(detail.mission.name)"))
                                    .padding(6)
                            }
                            .roomDrop(roomId, deps: deps, targeted: $dropping)
                            .animation(theme.motion.snappy, value: dropping)
                            .padding(.trailing, workspace.conversationVisible ? width + 6 : 0)
                            .transaction(value: workspace.conversationVisible) { $0.animation = nil }
                    }
                    if workspace.conversationVisible {
                        RoomConversationView(roomId: roomId, detail: detail, state: state, workspace: workspace)
                            .frame(width: narrow ? nil : width)
                            .overlay(alignment: .leading) {
                                if !narrow {
                                    resizeHandle(maximum: geometry.size.width - 380)
                                }
                            }
                            .padding([.trailing, .bottom], 6)
                            .transition(theme.motion.reduced ? .opacity : .scale(scale: 0.08, anchor: .topTrailing).combined(with: .opacity))
                    }
                }
            }
        }
        .animation(theme.motion.soft, value: workspace.conversationVisible)
    }

    private func resizeHandle(maximum: CGFloat) -> some View {
        Capsule().fill(theme.colors.inkFaint.opacity(handleHovered || dragStart != nil ? 0.7 : 0))
            .frame(width: 3, height: 34)
            .frame(width: 12).frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .offset(x: -9)
            .pointerStyle(.columnResize)
            .onHover { handleHovered = $0 }
            .animation(theme.motion.hover, value: handleHovered)
            .gesture(DragGesture(minimumDistance: 1, coordinateSpace: .global).onChanged { value in
                let start = dragStart ?? workspace.conversationWidth
                dragStart = start
                workspace.conversationWidth = min(max(start - value.translation.width, 300), max(300, min(540, maximum)))
            }.onEnded { _ in dragStart = nil })
            .accessibilityHidden(true)
    }
}

struct RoomHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: MissionDetail
    let state: RoomViewState
    @Bindable var workspace: RoomWorkspace
    @State private var name = ""
    @State private var confirmDelete = false
    @State private var confirmLeave = false
    @FocusState private var nameFocused: Bool

    private var roomId: String {
        detail.mission.id
    }

    private var isOwner: Bool {
        deps.rooms.isOwner(of: roomId)
    }

    private var sessions: [RuntimeSession] {
        deps.roomSessions(roomId)
    }

    var body: some View {
        HStack(spacing: 12) {
            RoomSigil(key: roomId, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                if workspace.renaming {
                    TextField("Room name", text: $name)
                        .textFieldStyle(.plain).appTextStyle(.headline).focused($nameFocused).frame(maxWidth: 260)
                        .onSubmit { run { try await $0.rename(roomId, to: name) } }
                        .onExitCommand { workspace.renaming = false }
                        .accessibilityIdentifier("room.name.input")
                } else {
                    Text(detail.mission.name).appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                        .contentTransition(.interpolate)
                }
                HStack(spacing: 5) {
                    Image(systemName: "lock.fill").font(.system(size: 8, weight: .semibold))
                    Text(verbatim: "\(Counted.people(detail.members.count)) · \(Counted.terminals(sessions.count))").contentTransition(.numericText())
                }
                .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                .help("Kodosi encrypts everything in this room end to end.")
            }
            .layoutPriority(1)
            Color.clear.frame(minWidth: 8, maxWidth: .infinity).titleBarGap()
            SectionDock(items: dockItems, selection: workspace.shownSection, identifier: "room.section") { workspace.shownSection = $0 }
            Color.clear.frame(minWidth: 8, maxWidth: .infinity).titleBarGap()
            if let failure = state.failure ?? state.readFailure, !workspace.conversationVisible,
               deps.rooms.snapshots[roomId] != nil || workspace.shownSection == 0
            {
                NoticeDot(message: failure) { Task { await deps.rooms.read(roomId) } } dismiss: {
                    state.failure = nil
                    state.readFailure = nil
                }
            }
            Button { workspace.peopleVisible.toggle() } label: {
                AvatarStack(people: detail.members.map { member in
                    AvatarStack.Person(id: member.userId, name: member.personName,
                                       isSelf: member.userId == deps.account.userId)
                }, size: 24, limit: 4, ring: theme.colors.surface)
            }
            .buttonStyle(PressScaleStyle()).help("People").accessibilityLabel(Text("Room members"))
            .accessibilityIdentifier("room.people")
            .kodosiPopover(isPresented: $workspace.peopleVisible, arrowEdge: .bottom) {
                RoomPeoplePopover(detail: detail, state: state, workspace: workspace)
            }
            if workspace.conversationVisible {
                IconButton(title: "Hide conversation", symbol: "bubble.left.and.bubble.right.fill",
                           identifier: "room.conversation.toggle", size: 30, active: true)
                {
                    workspace.conversationVisible = false
                }
                .transition(.opacity)
            } else {
                ConversationKey(roomId: roomId, state: state, workspace: workspace)
                    .transition(theme.motion.reduced ? .opacity : .scale(scale: 0.6, anchor: .trailing).combined(with: .opacity))
            }
            IconButton(title: "Room options", symbol: "ellipsis", identifier: "room.options", size: 30, open: workspace.optionsVisible) {
                workspace.optionsVisible.toggle()
            }
            .kodosiPopover(isPresented: $workspace.optionsVisible, arrowEdge: .bottom, alignment: .trailing) {
                VStack(alignment: .leading, spacing: 2) {
                    if isOwner {
                        PanelMenuRow(Text("Rename"), icon: .symbol("pencil"), identifier: "room.options.rename") {
                            name = detail.mission.name; workspace.renaming = true; nameFocused = true
                        }
                    }
                    PanelMenuRow(Text("Refresh"), icon: .symbol("arrow.clockwise"), identifier: "room.options.refresh") {
                        Task { await deps.rooms.read(roomId) }
                    }
                    PanelMenuDivider()
                    if isOwner {
                        PanelMenuRow(Text("Delete room…"), icon: .symbol("trash"), identifier: "room.options.delete", destructive: true) {
                            confirmDelete = true
                        }
                    } else {
                        PanelMenuRow(Text("Leave room…"), icon: .symbol("rectangle.portrait.and.arrow.right"),
                                     identifier: "room.options.leave", destructive: true)
                        {
                            confirmLeave = true
                        }
                    }
                }
                .padding(6)
                .panelMenuWidth(200)
            }
        }
        .padding(.leading, 16).padding(.trailing, 10).frame(height: Metrics.headerHeight)
        .animation(theme.motion.snappy, value: state.unread)
        .animation(theme.motion.soft, value: workspace.conversationVisible)
        .confirmationDialog("Delete \(detail.mission.name)?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete room", role: .destructive) { end { try await $0.delete(roomId) } }
        } message: { Text("Its conversation and sharing end. Terminals keep running.") }
        .confirmationDialog("Leave \(detail.mission.name)?", isPresented: $confirmLeave, titleVisibility: .visible) {
            Button("Leave room", role: .destructive) { end { try await $0.leave(roomId) } }
        } message: { Text("You lose its terminals and conversation until someone invites you again.") }
    }

    private var dockItems: [DockItem] {
        let tasks = deps.rooms.snapshots[roomId]?.tasks ?? []
        let open = tasks.filter { !$0.closed }.count
        return [
            DockItem(id: 0, title: String(localized: "Terminals"), symbol: "apple.terminal", count: sessions.count),
            DockItem(id: 1, title: String(localized: "Tasks"), symbol: "checklist", count: open,
                     progress: tasks.isEmpty ? nil : Double(tasks.count - open) / Double(tasks.count), fresh: state.fresh.contains(.tasks)),
            DockItem(id: 2, title: String(localized: "Repositories"), symbol: "shippingbox",
                     count: deps.rooms.snapshots[roomId]?.repositories.count ?? 0, fresh: state.fresh.contains(.repositories)),
        ]
    }

    private func run(_ work: @escaping (RoomStore) async throws -> Void) {
        let rooms = deps.rooms
        Task { @MainActor in
            do {
                try await work(rooms)
                workspace.renaming = false
            } catch { state.failure = error.localizedDescription }
        }
    }

    private func end(_ work: @escaping (RoomStore) async throws -> Void) {
        run { rooms in
            try await work(rooms)
            deps.workbench.selectedMissionId = nil
        }
    }
}

struct NoticeDot: View {
    @Environment(\.theme) private var theme
    @State private var expanded = false
    let message: String
    let retry: () -> Void
    let dismiss: () -> Void

    var body: some View {
        Button { expanded.toggle() } label: {
            Circle().fill(theme.colors.caution).frame(width: 9, height: 9).frame(width: 26, height: 26)
                .background(theme.colors.cautionSoft, in: Circle())
        }
        .buttonStyle(PressScaleStyle()).help("Something did not work").accessibilityLabel(Text("Room action failed"))
        .accessibilityIdentifier("room.failure")
        .kodosiPopover(isPresented: $expanded, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 14) {
                Text(message).appTextStyle(.body).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                HStack {
                    Button("Dismiss") { expanded = false; dismiss() }.buttonStyle(.kodosi(.ghost, size: .small))
                    Spacer()
                    Button("Try again") { expanded = false; retry() }.buttonStyle(.kodosi(.primary, size: .small))
                }
            }
            .padding(16).popoverSheet(width: 320)
        }
    }
}

struct RoomPeoplePopover: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: MissionDetail
    let state: RoomViewState
    let workspace: RoomWorkspace
    @State private var invited: Set<String> = []

    private var isOwner: Bool {
        deps.rooms.isOwner(of: detail.mission.id)
    }

    private var candidates: [FriendEntry] {
        deps.rooms.invitable(deps.people.friends, to: detail.mission.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text("In this room").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint).padding(.leading, 8)
                ForEach(detail.members, id: \.userId) { member in memberRow(member) }
            }
            if isOwner {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Invite").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint).padding(.leading, 8)
                    if candidates.isEmpty {
                        Button { workspace.peopleVisible = false; deps.workbench.section = .people } label: {
                            MenuRow(title: String(localized: "Add friends"), symbol: "person.badge.plus")
                        }.buttonStyle(.plain)
                    }
                    ForEach(candidates) { friend in
                        let name = friend.personName
                        let sent = invited.contains(friend.userId)
                        HStack(spacing: 10) {
                            PersonAvatar(name: name, key: friend.userId, size: 26)
                            Text(name).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                            Spacer(minLength: 8)
                            Button(sent ? "Invited" : "Invite") { invite(friend) }
                                .buttonStyle(.kodosi(sent ? .ghost : .tinted, size: .small)).disabled(sent || friend.identityChanged)
                                .accessibilityIdentifier("room.invite.\(friend.handle)")
                        }
                        .padding(.horizontal, 8).frame(height: 38)
                        .help(friend.identityChanged ? String(localized: "Their identity changed. Trust them in Friends first.") : "")
                    }
                }
            }
        }
        .padding(10)
        .popoverSheet(width: 300)
        .animation(theme.motion.snappy, value: invited)
    }

    private func memberRow(_ member: MissionMemberEntry) -> some View {
        let name = member.personName
        return HStack(spacing: 10) {
            PersonAvatar(name: name, key: member.userId, size: 26, isSelf: member.userId == deps.account.userId)
            VStack(alignment: .leading, spacing: 0) {
                Text(member.listedName(you: deps.account.userId))
                    .appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                Text(verbatim: "@\(member.handle)").appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
            }
            Spacer(minLength: 8)
            if member.isOwner {
                Tag(text: String(localized: "Owner"))
            } else if isOwner {
                Menu {
                    Button("Remove from room", role: .destructive) { remove(member) }
                } label: {
                    Image(systemName: "ellipsis").font(.system(size: 11, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                        .frame(width: 24, height: 24).contentShape(Circle())
                }
                .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
                .accessibilityLabel(Text("Options for \(name)"))
            }
        }
        .padding(.horizontal, 8).frame(height: 40)
    }

    private func invite(_ friend: FriendEntry) {
        invited.insert(friend.userId)
        Task { @MainActor in
            do {
                try await deps.rooms.invite(friend.userId, to: detail.mission.id)
            } catch {
                invited.remove(friend.userId)
                state.failure = error.localizedDescription
            }
        }
    }

    private func remove(_ member: MissionMemberEntry) {
        Task { @MainActor in
            do {
                try await deps.rooms.remove(member, from: detail.mission.id)
            } catch { state.failure = error.localizedDescription }
        }
    }
}

struct ConversationKey: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let roomId: String
    let state: RoomViewState
    let workspace: RoomWorkspace

    private var last: RoomMessage? {
        deps.rooms.snapshots[roomId]?.messages.last
    }

    var body: some View {
        Button { workspace.conversationVisible = true } label: {
            HStack(spacing: 6) {
                face.id(last?.id)
                    .transition(theme.motion.reduced ? .opacity : .scale(scale: 0.7).combined(with: .opacity))
                if state.unread > 0 {
                    Text(state.unread, format: .number).appTextStyle(.caption).monospacedDigit()
                        .foregroundStyle(theme.colors.accentStrong).contentTransition(.numericText())
                        .transition(AnyTransition.pop)
                }
            }
            .padding(.leading, 5).padding(.trailing, state.unread > 0 ? 9 : 5).frame(height: 30)
            .background {
                RaisedBackground(shape: Capsule(), fill: hovered ? theme.colors.lifted : theme.colors.raised,
                                 elevation: hovered ? .lifted : .resting)
            }
            .contentShape(Capsule())
        }
        .buttonStyle(PressScaleStyle())
        .onHover { hovered = $0 }
        .animation(theme.motion.snappy, value: last?.id)
        .animation(theme.motion.snappy, value: state.unread)
        .animation(theme.motion.hover, value: hovered)
        .help(last.map { String(localized: "Show conversation. Last message from \($0.agent ?? $0.authorName)") } ?? String(localized: "Show conversation"))
        .accessibilityLabel(Text("Show conversation"))
        .accessibilityValue(state.unread > 0 ? Text("\(state.unread) new") : Text(verbatim: ""))
        .accessibilityIdentifier("room.conversation.toggle")
    }

    @ViewBuilder
    private var face: some View {
        if let last, let terminal = last.terminalId.flatMap(deps.session) {
            AgentMark(session: terminal, size: 20, activity: terminal.mark(rested: .awake))
        } else if let last {
            PersonAvatar(name: last.agent ?? last.authorName, key: last.authorId, size: 20,
                         isSelf: last.isWritten(by: deps.account.userId))
        } else {
            Image(systemName: "bubble.left.and.bubble.right").font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.colors.inkMuted).frame(width: 20, height: 20)
        }
    }
}
