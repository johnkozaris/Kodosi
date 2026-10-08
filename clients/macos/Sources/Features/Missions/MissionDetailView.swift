import SwiftUI

struct MissionDetailView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let detail: MissionDetail
    @Bindable var state: RoomViewState
    @State private var peopleVisible = false
    @State private var renaming = false
    @State private var name = ""
    @State private var confirmDelete = false
    private var roomId: String {
        detail.mission.id
    }

    private var isOwner: Bool {
        detail.mission.ownerUserId == deps.userId
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            GeometryReader { geometry in
                if geometry.size.width < 720 {
                    if state.conversationVisible {
                        RoomConversationView(roomId: roomId, state: state)
                    } else {
                        RoomContentView(roomId: roomId, state: state)
                    }
                } else {
                    HSplitView {
                        RoomContentView(roomId: roomId, state: state).frame(minWidth: 370)
                        if state.conversationVisible {
                            RoomConversationView(roomId: roomId, state: state).frame(minWidth: 300, idealWidth: 380, maxWidth: 500)
                                .transition(.move(edge: .trailing).combined(with: .opacity))
                        }
                    }
                }
            }
        }.animation(reduceMotion ? nil : .spring(response: 0.32, dampingFraction: 0.88), value: state.conversationVisible)
            .task(id: roomId) { await deps.readRoom(roomId) }
            .confirmationDialog("Delete \(detail.mission.name)?", isPresented: $confirmDelete, titleVisibility: .visible) {
                Button("Delete room", role: .destructive) { run("mission.delete") }
            } message: { Text("Room history and sharing will end. Terminals keep running.") }
    }

    private var header: some View {
        HStack(spacing: 12) {
            Text(detail.mission.name).font(.system(size: 16, weight: .semibold)).lineLimit(1)
            Spacer(minLength: 8)
            if let failure = state.failure {
                Menu { Text(failure); Button("Retry") { Task { await deps.readRoom(roomId) } }; Button("Dismiss") { state.failure = nil } }
                    label: { Image(systemName: "exclamationmark.circle.fill").foregroundStyle(theme.colors.destructive) }
                    .menuStyle(.borderlessButton).frame(width: 20).accessibilityLabel(Text("Room action failed"))
            }
            Button { peopleVisible.toggle() } label: {
                HStack(spacing: -6) {
                    ForEach(Array(detail.members.prefix(4)), id: \.userId) { member in
                        RoomAvatar(name: member.displayName.isEmpty ? member.handle : member.displayName)
                            .overlay { Circle().stroke(theme.colors.surfacePanel, lineWidth: 2) }
                    }
                    if detail.members.count > 4 {
                        Text("+\(detail.members.count - 4)").font(.system(size: 11)).padding(.leading, 10)
                    }
                }
            }.buttonStyle(.plain).help("People").accessibilityLabel(Text("Room members"))
                .accessibilityIdentifier("room.people").popover(isPresented: $peopleVisible) { people }
            Menu {
                ForEach(deps.sessions.filter { $0.isOwner && $0.missionId != roomId }) { session in
                    Button(session.name) { share(session) }
                }
                Divider()
                Button("New terminal") { deps.newRoomTerminal(roomId) }
            } label: { Image(systemName: "plus.rectangle.on.rectangle") }
                .menuStyle(.borderlessButton).frame(width: 24).help("Add terminal").accessibilityLabel(Text("Add terminal"))
                .accessibilityIdentifier("room.terminal.add")
            Button {
                state.conversationVisible.toggle()
                if state.conversationVisible {
                    state.unread = 0
                }
            } label: {
                Image(systemName: state.conversationVisible ? "bubble.left.fill" : "bubble.left")
                    .foregroundStyle(state.conversationVisible ? theme.colors.primary : theme.colors.mutedForeground)
                    .overlay(alignment: .topTrailing) {
                        if state.unread > 0 {
                            Circle().fill(theme.colors.primary).frame(width: 6, height: 6).offset(x: 4, y: -4)
                        }
                    }
            }.buttonStyle(.plain).help("Toggle conversation").accessibilityLabel(Text("Toggle conversation"))
                .accessibilityIdentifier("room.conversation.toggle")
            Menu {
                if isOwner {
                    Button("Rename room") { name = detail.mission.name; renaming = true }
                    Button("Delete room…", role: .destructive) { confirmDelete = true }
                } else {
                    Button("Leave room") { run("mission.leave") }
                }
                Button("Refresh") { Task { await deps.readRoom(roomId) } }
            } label: { Image(systemName: "ellipsis") }
                .menuStyle(.borderlessButton).frame(width: 24).accessibilityLabel(Text("Room options"))
                .popover(isPresented: $renaming) {
                    HStack {
                        TextField("Room name", text: $name)
                        Button("Rename") { run("mission.rename", ["name": .string(name)]) }.disabled(!ProductInput.validName(name))
                    }.padding(16).frame(width: 330)
                }
        }.padding(.horizontal, 18).frame(height: 56).background(theme.colors.surfacePanel).seamBorder(.bottom)
    }

    private var people: some View {
        VStack(alignment: .leading, spacing: 14) {
            ForEach(detail.members, id: \.userId) { member in
                HStack(spacing: 10) {
                    RoomAvatar(name: member.displayName)
                    Text(member.displayName.isEmpty ? member.handle : member.displayName).font(.system(size: 13))
                    Spacer()
                    if isOwner, !member.isOwner {
                        Menu { Button("Remove from room", role: .destructive) { run("mission.removeMember", ["userId": .string(member.userId)]) } }
                            label: { Image(systemName: "ellipsis") }.menuStyle(.borderlessButton).frame(width: 20)
                    }
                }
            }
            if isOwner {
                Divider()
                Menu {
                    ForEach(deps.friends.filter { friend in !detail.members.contains { $0.userId == friend.userId } }) { friend in
                        Button(friend.displayName) { run("mission.invite", ["userId": .string(friend.userId)]) }
                    }
                } label: { Label("Invite", systemImage: "person.badge.plus") }
            }
        }.padding(18).frame(width: 270)
    }

    private func share(_ session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await deps.mutateSession("session.attachMission", session: session, fields: ["missionId": .string(roomId)])
                state.canvas = 0; deps.activateSession(session.id, inRoom: roomId)
            } catch { state.failure = error.localizedDescription }
        }
    }

    private func run(_ operation: String, _ fields: [String: JSONValue] = [:]) {
        var fields = fields; fields["missionId"] = .string(roomId)
        Task { @MainActor in
            do {
                _ = try await deps.commandSink.request(operation, fields)
                renaming = false
                if operation == "mission.delete" || operation == "mission.leave" {
                    deps.workbench.selectedMissionId = nil; deps.missionDetail = nil
                }
            } catch { state.failure = error.localizedDescription }
        }
    }
}
