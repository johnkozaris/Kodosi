import KodosiBridge
import KodosiDesign
import SwiftUI

struct MissionsView: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        if !deps.account.isReady {
            SignInView(
                title: "Rooms are for working together",
                message: "Share terminals, talk, and hand off tasks with people and agents.",
            )
        } else if let detail = deps.missionDetail, detail.mission.id == deps.workbench.selectedMissionId {
            RoomView(detail: detail, state: deps.roomView(detail.mission.id))
                .id(detail.mission.id)
                .transition(.opacity)
        } else if deps.workbench.selectedMissionId != nil {
            RoomSkeleton()
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
                PageHeader(title: "Rooms", facts: facts) {
                    Button { deps.workbench.showsNewRoom = true } label: { Label("New room", systemImage: "plus") }
                        .buttonStyle(.kodosi(.primary)).accessibilityIdentifier("missions.create.lobby")
                }.arrive()
                if !deps.invitations.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(deps.invitations) { invitation in
                            InvitationCard(invitation: invitation, identifier: "missions.invitation.\(AccessibilityIdentifier.token(invitation.missionId))") {
                                deps.respond(to: invitation, accept: false)
                            } join: {
                                deps.respond(to: invitation, accept: true)
                            }
                        }
                    }.arrive(delay: 0.04)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 250, maximum: 340), spacing: 14)], alignment: .leading, spacing: 14) {
                    ForEach(Array(deps.missions.enumerated()), id: \.element.id) { index, mission in
                        RoomCard(mission: mission, sessions: deps.roomSessions(mission.id), yours: mission.ownerUserId == deps.account.userId) {
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
                                Text(deps.missions.isEmpty ? "Make your first room" : "New room").appTextStyle(.subhead)
                            }
                            .foregroundStyle(theme.colors.inkMuted)
                            .frame(maxWidth: .infinity).frame(height: 132)
                        }
                    }
                    .buttonStyle(PressScaleStyle(scale: 0.98)).accessibilityIdentifier("missions.create.slot")
                    .arrive(delay: 0.04 * Double(min(deps.missions.count, 8) + 1))
                }
            }
            .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 36)
            .frame(maxWidth: 1100, alignment: .leading).frame(maxWidth: .infinity)
        }
    }

    private var facts: String? {
        guard !deps.missions.isEmpty else { return String(localized: "A room keeps people, agents and terminals in one place.") }
        let shared = deps.sessions.count { $0.missionId != nil && $0.status != .closing }
        return "\(Counted.rooms(deps.missions.count)) · " + String(localized: "\(shared) shared terminals")
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
                Text("Name your room").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Text("Invite friends after. Your terminals in the room go to your friends.")
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
                    Text(creating ? "Making…" : "Make room").frame(maxWidth: .infinity).shimmer(creating)
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
                let id = UUIDv7.generate()
                _ = try await deps.commandSink.request("mission.create", [
                    "requestId": .string(id),
                    "name": .string(name.trimmingCharacters(in: .whitespacesAndNewlines)),
                ])
                deps.workbench.showsNewRoom = false
                name = ""
                deps.workbench.section = .missions
                deps.openMission(id: id)
            } catch { self.error = error.localizedDescription }
        }
    }
}
