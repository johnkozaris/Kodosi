import KodosiBridge
import KodosiDesign
import SwiftUI

struct MissionsView: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        if !deps.account.isReady {
            SignInView(
                title: "Work in projects",
                message: "A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.",
            )
        } else if let roomId = deps.workbench.selectedMissionId {
            Group {
                if let detail = deps.rooms.details[roomId] {
                    RoomView(detail: detail, state: deps.rooms.view(roomId), workspace: deps.roomWorkspace(roomId))
                        .id(roomId)
                        .transition(.opacity)
                } else if let failure = deps.rooms.detailFailures[roomId] {
                    RoomRecovery(message: failure) { Task { await deps.rooms.open(roomId) } }
                } else {
                    RoomSkeleton()
                }
            }
            .showsRoom(roomId, in: deps.rooms)
        } else {
            RoomsLobby()
        }
    }
}

struct RoomsLobby: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                PageHeader(title: "Projects", facts: facts) {
                    Button { deps.workbench.showsNewRoom = true } label: { Label("New project", systemImage: "plus") }
                        .buttonStyle(.kodosi(.primary)).accessibilityIdentifier("missions.create.lobby")
                }.arrive()
                if !deps.rooms.invitations.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(deps.rooms.invitations) { invitation in
                            InvitationCard(invitation: invitation, identifier: "missions.invitation.\(AccessibilityIdentifier.token(invitation.missionId))") {
                                deps.respond(to: invitation, accept: false)
                            } join: {
                                deps.respond(to: invitation, accept: true)
                            }
                        }
                    }.arrive(delay: 0.04)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 250, maximum: 340), spacing: 14)], alignment: .leading, spacing: 14) {
                    ForEach(Array(deps.rooms.missions.enumerated()), id: \.element.id) { index, mission in
                        RoomCard(mission: mission, sessions: deps.roomSessions(mission.id), yours: deps.rooms.isOwner(of: mission.id)) {
                            deps.openMission(mission)
                        } detail: {
                            EmptyView()
                        }
                        .accessibilityIdentifier("missions.card.\(AccessibilityIdentifier.token(mission.id))")
                        .arrive(delay: 0.04 * Double(min(index, 8) + 1))
                    }
                    Button { deps.workbench.showsNewRoom = true } label: {
                        DashedSlot(radius: Radius.xl) {
                            VStack(spacing: 10) {
                                Image(systemName: "plus").font(.system(size: 18, weight: .semibold))
                                Text(deps.rooms.missions.isEmpty ? "Make your first project" : "New project").appTextStyle(.subhead)
                            }
                            .foregroundStyle(theme.colors.inkMuted)
                            .frame(maxWidth: .infinity).frame(height: 132)
                        }
                    }
                    .buttonStyle(PressScaleStyle(scale: 0.98)).accessibilityIdentifier("missions.create.slot")
                    .arrive(delay: 0.04 * Double(min(deps.rooms.missions.count, 8) + 1))
                }
            }
            .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 36)
            .frame(maxWidth: 1100, alignment: .leading).frame(maxWidth: .infinity)
        }
    }

    private var facts: String? {
        guard !deps.rooms.missions.isEmpty else { return String(localized: "A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.") }
        let shared = deps.sessions.count { $0.missionId != nil && $0.status != .closing }
        return "\(Counted.rooms(deps.rooms.missions.count)) · " + String(localized: "\(shared) shared terminals")
    }
}

struct NewRoomSheet: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var name = ""
    @State private var creating = false
    @State private var error: String?
    @FocusState private var focused: Bool

    private var valid: Bool {
        ProductInput.validName(name)
    }

    var body: some View {
        VStack(spacing: 22) {
            RoomSigil(key: name.isEmpty ? "new" : name, size: 64)
                .animation(theme.motion.spring, value: name)
            VStack(spacing: 6) {
                Text("Name your project").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Text("Invite friends after. Your terminals in the project go to your friends.")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
            }
            TextField("Launch week", text: $name)
                .textFieldStyle(WellTextFieldStyle(radius: Radius.lg)).focused($focused)
                .onSubmit(create).accessibilityIdentifier("missions.create.name")
            if let error {
                ErrorNote(message: error)
            }
            HStack(spacing: 8) {
                Button("Cancel") { deps.workbench.showsNewRoom = false }.keyboardShortcut(.cancelAction).buttonStyle(.kodosi(.ghost, size: .large))
                Button(action: create) {
                    Text(creating ? "Making…" : "Make project").frame(maxWidth: .infinity).shimmer(creating)
                }
                .buttonStyle(.kodosi(.primary, size: .large)).disabled(creating || !valid)
                .accessibilityIdentifier("missions.create.confirm")
            }
        }
        .padding(28).frame(width: 400)
        .onAppear { focused = true }
    }

    private func create() {
        guard valid, !creating else { return }
        creating = true
        error = nil
        Task { @MainActor in
            defer { creating = false }
            do {
                let id = try await deps.rooms.create(name: name, inviting: [])
                deps.workbench.showsNewRoom = false
                name = ""
                deps.workbench.section = .missions
                deps.openMission(id: id)
            } catch { self.error = error.localizedDescription }
        }
    }
}
