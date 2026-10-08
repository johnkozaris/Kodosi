import SwiftUI

struct RoomView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: MissionDetail
    @Bindable var state: RoomViewState
    @State private var dragStart: CGFloat?
    @State private var handleHovered = false

    private var roomId: String {
        detail.mission.id
    }

    var body: some View {
        VStack(spacing: 0) {
            RoomHeader(detail: detail, state: state)
            GeometryReader { geometry in
                let narrow = geometry.size.width < 720
                let width = min(max(state.conversationWidth, 300), max(300, min(540, geometry.size.width - 380)))
                ZStack(alignment: .trailing) {
                    if !(narrow && state.conversationVisible) {
                        RoomContentView(roomId: roomId, state: state)
                            .padding(.trailing, state.conversationVisible ? width + 6 : 0)
                            .transaction(value: state.conversationVisible) { $0.animation = nil }
                    }
                    if state.conversationVisible {
                        RoomConversationView(roomId: roomId, detail: detail, state: state)
                            .frame(width: narrow ? nil : width)
                            .overlay(alignment: .leading) {
                                if !narrow {
                                    resizeHandle(maximum: geometry.size.width - 380)
                                }
                            }
                            .padding([.trailing, .bottom], 6)
                            .transition(.move(edge: .trailing).combined(with: .opacity))
                    }
                }
            }
        }
        .animation(theme.motion.soft, value: state.conversationVisible)
        .task(id: roomId) { await deps.readRoom(roomId) }
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
                let start = dragStart ?? state.conversationWidth
                dragStart = start
                state.conversationWidth = min(max(start - value.translation.width, 300), max(300, min(540, maximum)))
            }.onEnded { _ in dragStart = nil })
            .accessibilityHidden(true)
    }
}

struct RoomHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: MissionDetail
    @Bindable var state: RoomViewState
    @State private var name = ""
    @State private var confirmDelete = false
    @State private var confirmLeave = false
    @FocusState private var nameFocused: Bool

    private var roomId: String {
        detail.mission.id
    }

    private var isOwner: Bool {
        detail.mission.ownerUserId == deps.userId
    }

    private var sessions: [RuntimeSession] {
        deps.sessions.filter { $0.missionId == roomId }
    }

    var body: some View {
        HStack(spacing: 12) {
            RoomSigil(name: detail.mission.name, key: roomId, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                if state.renaming {
                    TextField("Room name", text: $name)
                        .textFieldStyle(.plain).appTextStyle(.headline).focused($nameFocused).frame(maxWidth: 260)
                        .onSubmit { run("mission.rename", ["name": .string(name)]) }
                        .onExitCommand { state.renaming = false }
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
                .help("Everything in this room is encrypted end to end.")
            }
            .layoutPriority(1)
            Spacer(minLength: 8)
            SectionDock(items: dockItems, selection: state.canvas, identifier: "room.canvas") { state.canvas = $0 }
            Spacer(minLength: 8)
            if let failure = state.failure {
                NoticeDot(message: failure) { Task { await deps.readRoom(roomId) } } dismiss: { state.failure = nil }
            }
            Button { state.peopleVisible.toggle() } label: {
                AvatarStack(people: detail.members.map { member in
                    AvatarStack.Person(id: member.userId, name: member.displayName.isEmpty ? member.handle : member.displayName,
                                       isSelf: member.userId == deps.userId)
                }, size: 24, limit: 4, ring: theme.colors.surface)
            }
            .buttonStyle(PressScaleStyle()).help("People").accessibilityLabel(Text("Room members"))
            .accessibilityIdentifier("room.people")
            .popover(isPresented: $state.peopleVisible, arrowEdge: .bottom) { RoomPeoplePopover(detail: detail, state: state) }
            IconButton(title: state.conversationVisible ? "Hide conversation" : "Show conversation",
                       symbol: state.conversationVisible ? "bubble.left.and.bubble.right.fill" : "bubble.left.and.bubble.right",
                       identifier: "room.conversation.toggle", size: 30, active: state.conversationVisible)
            {
                state.conversationVisible.toggle()
                if state.conversationVisible {
                    state.unread = 0
                }
            }
            .overlay(alignment: .topTrailing) {
                if state.unread > 0, !state.conversationVisible {
                    CountBadge(count: state.unread).offset(x: 6, y: -4).transition(AnyTransition.pop)
                }
            }
            Menu {
                if isOwner {
                    Button("Rename") { name = detail.mission.name; state.renaming = true; nameFocused = true }
                }
                Button("Refresh") { Task { await deps.readRoom(roomId) } }
                Divider()
                if isOwner {
                    Button("Delete room…", role: .destructive) { confirmDelete = true }
                } else {
                    Button("Leave room…", role: .destructive) { confirmLeave = true }
                }
            } label: {
                Image(systemName: "ellipsis").font(.system(size: 13, weight: .semibold)).foregroundStyle(theme.colors.inkMuted)
                    .frame(width: 30, height: 30).contentShape(Circle())
            }
            .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            .accessibilityLabel(Text("Room options")).accessibilityIdentifier("room.options")
        }
        .padding(.leading, 16).padding(.trailing, 10).frame(height: Metrics.headerHeight)
        .animation(theme.motion.snappy, value: state.unread)
        .confirmationDialog("Delete \(detail.mission.name)?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete room", role: .destructive) { run("mission.delete") }
        } message: { Text("Its conversation and sharing end. Terminals keep running.") }
        .confirmationDialog("Leave \(detail.mission.name)?", isPresented: $confirmLeave, titleVisibility: .visible) {
            Button("Leave room", role: .destructive) { run("mission.leave") }
        } message: { Text("You lose its terminals and conversation until someone invites you again.") }
    }

    private var dockItems: [DockItem] {
        let tasks = deps.rooms[roomId]?.tasks ?? []
        let open = tasks.filter { !$0.closed }.count
        return [
            DockItem(id: 0, title: String(localized: "Terminals"), symbol: "apple.terminal", count: sessions.count),
            DockItem(id: 1, title: String(localized: "Tasks"), symbol: "checklist", count: open,
                     progress: tasks.isEmpty ? nil : Double(tasks.count - open) / Double(tasks.count), fresh: state.fresh.contains(1)),
            DockItem(id: 2, title: String(localized: "Repositories"), symbol: "shippingbox",
                     count: deps.rooms[roomId]?.repositories.count ?? 0, fresh: state.fresh.contains(2)),
        ]
    }

    private func run(_ operation: String, _ fields: [String: JSONValue] = [:]) {
        var fields = fields; fields["missionId"] = .string(roomId)
        Task { @MainActor in
            do {
                _ = try await deps.commandSink.request(operation, fields)
                state.renaming = false
                if operation == "mission.delete" || operation == "mission.leave" {
                    deps.workbench.selectedMissionId = nil; deps.missionDetail = nil
                }
            } catch { state.failure = error.localizedDescription }
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
        .popover(isPresented: $expanded, arrowEdge: .bottom) {
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
    @Bindable var state: RoomViewState
    @State private var invited: Set<String> = []

    private var isOwner: Bool {
        detail.mission.ownerUserId == deps.userId
    }

    private var candidates: [FriendEntry] {
        deps.friends.filter { friend in !detail.members.contains { $0.userId == friend.userId } }
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
                        Button { state.peopleVisible = false; deps.workbench.section = .people } label: {
                            MenuRow(title: String(localized: "Add friends in People"), symbol: "person.badge.plus")
                        }.buttonStyle(.plain)
                    }
                    ForEach(candidates) { friend in
                        let name = friend.displayName.isEmpty ? friend.handle : friend.displayName
                        let sent = invited.contains(friend.userId)
                        HStack(spacing: 10) {
                            PersonAvatar(name: name, key: friend.userId, size: 26)
                            Text(name).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                            Spacer(minLength: 8)
                            Button(sent ? "Invited" : "Invite") { invite(friend) }
                                .buttonStyle(.kodosi(sent ? .ghost : .tinted, size: .small)).disabled(sent)
                                .accessibilityIdentifier("room.invite.\(friend.handle)")
                        }
                        .padding(.horizontal, 8).frame(height: 38)
                    }
                }
            }
        }
        .padding(10)
        .popoverSheet(width: 300)
        .animation(theme.motion.snappy, value: invited)
    }

    private func memberRow(_ member: MissionMemberEntry) -> some View {
        let name = member.displayName.isEmpty ? member.handle : member.displayName
        return HStack(spacing: 10) {
            PersonAvatar(name: name, key: member.userId, size: 26, isSelf: member.userId == deps.userId)
            VStack(alignment: .leading, spacing: 0) {
                Text(member.userId == deps.userId ? String(localized: "\(name) (you)") : name)
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
                _ = try await deps.commandSink.request("mission.invite", ["missionId": .string(detail.mission.id), "userId": .string(friend.userId)])
            } catch {
                invited.remove(friend.userId)
                state.failure = error.localizedDescription
            }
        }
    }

    private func remove(_ member: MissionMemberEntry) {
        Task { @MainActor in
            do {
                _ = try await deps.commandSink.request(
                    "mission.removeMember", ["missionId": .string(detail.mission.id), "userId": .string(member.userId)]
                )
            } catch { state.failure = error.localizedDescription }
        }
    }
}
