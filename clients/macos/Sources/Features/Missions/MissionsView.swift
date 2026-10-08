import SwiftUI

struct MissionsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        if !deps.accountReady {
            SignInView(
                title: "Rooms are for working together",
                message: "Share terminals, talk, and hand off tasks with people and agents."
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
                        ForEach(deps.invitations) { invitation in InvitationCard(invitation: invitation) }
                    }.arrive(delay: 0.04)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 250, maximum: 340), spacing: 14)], alignment: .leading, spacing: 14) {
                    ForEach(Array(deps.missions.enumerated()), id: \.element.id) { index, mission in
                        RoomCard(mission: mission).arrive(delay: 0.04 * Double(min(index, 8) + 1))
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
        let shared = deps.sessions.filter { $0.missionId != nil }.count
        return "\(Counted.rooms(deps.missions.count)) · " + String(localized: "\(shared) shared terminals")
    }
}

private struct RoomCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let mission: MissionEntry

    private var sessions: [RuntimeSession] {
        deps.sessions.filter { $0.missionId == mission.id }
    }

    var body: some View {
        let unread = deps.roomViews[mission.id]?.unread ?? 0
        Button { deps.openMission(mission) } label: {
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top) {
                    RoomSigil(name: mission.name, key: mission.id, size: 44)
                    Spacer()
                    if unread > 0 {
                        CountBadge(count: unread)
                    } else if mission.ownerUserId == deps.userId {
                        Tag(text: String(localized: "Yours"))
                    }
                }
                Spacer(minLength: 14)
                Text(mission.name).appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                HStack(spacing: 6) {
                    if sessions.isEmpty {
                        Text("No terminals yet").appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                    } else {
                        HStack(spacing: -4) {
                            ForEach(sessions.prefix(5)) { session in
                                AgentMark(kind: session.agent, size: 18, activity: session.isWorking ? .working : .awake)
                            }
                        }
                        Text(sessions.contains(where: \.isWorking) ? String(localized: "Working") : Counted.terminals(sessions.count))
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .shimmer(sessions.contains(where: \.isWorking))
                    }
                }
                .padding(.top, 6).frame(height: 24)
            }
            .padding(16).frame(maxWidth: .infinity, alignment: .leading).frame(height: 132)
            .raised(Radius.xl, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
            .offset(y: hovered ? -2 : 0)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .onHover { hovered = $0 }
        .animation(theme.motion.spring, value: hovered)
        .accessibilityIdentifier("missions.card.\(AccessibilityIdentifier.token(mission.id))")
    }
}

private struct InvitationCard: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let invitation: MissionInvitationEntry

    var body: some View {
        HStack(spacing: 14) {
            RoomSigil(name: invitation.missionName, key: invitation.missionId, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(invitation.missionName).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text("\(invitation.inviterName) invited you").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
            Spacer()
            Button("Not now") { deps.respond(to: invitation, accept: false) }.buttonStyle(.kodosi(.ghost))
            Button("Join") { deps.respond(to: invitation, accept: true) }.buttonStyle(.kodosi(.primary))
        }
        .padding(14)
        .raised(Radius.xl)
        .overlay { RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(0.6), lineWidth: 1.5) }
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
            RoomSigil(name: name, key: name.isEmpty ? "new" : name, size: 64)
                .animation(theme.motion.spring, value: name)
            VStack(spacing: 6) {
                Text("Name your room").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Text("Invite people after. Everyone in the room shares its terminals.")
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
        .padding(28).frame(width: 400).background(theme.colors.raised.mix(with: theme.colors.surface, by: 0.4))
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
