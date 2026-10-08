import SwiftUI

struct SessionSharingPopover: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let session: RuntimeSession
    @State private var people: Set<String> = []
    @State private var originalPeople: Set<String> = []
    @State private var room: String?
    @State private var saving = false
    @State private var error: String?

    private var current: RuntimeSession {
        deps.session(session.id) ?? session
    }

    private var changed: Bool {
        room != current.missionId || people != originalPeople
    }

    private var actionTitle: String {
        if room != current.missionId {
            if let room, let name = deps.missions.first(where: { $0.id == room })?.name {
                return String(localized: "Share with \(name)")
            }
            return String(localized: "Stop sharing")
        }
        if !changed, room != nil || !people.isEmpty {
            return String(localized: "Shared")
        }
        if people.isEmpty {
            return originalPeople.isEmpty ? String(localized: "Share") : String(localized: "Stop sharing")
        }
        if people.count == 1, let id = people.first {
            return String(localized: "Share with \(deps.personName(id))")
        }
        return String(localized: "Share with \(people.count) people")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 11) {
                AgentMark(kind: session.agent, size: 34)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Share \(session.name)").appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                    Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer()
                IconButton(title: "Close", symbol: "xmark", identifier: "sharing.close") { dismiss() }
            }
            let here = deps.viewers(of: current)
            if !here.isEmpty {
                HStack(spacing: 8) {
                    AvatarStack(people: here, size: 22, ring: theme.colors.raised)
                    Text("Here now").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
            }
            if !deps.accountReady {
                Button(deps.signInPrompt ?? String(localized: "Sign in")) {
                    dismiss()
                    deps.beginSignIn()
                }.buttonStyle(.kodosi(.primary)).frame(maxWidth: .infinity)
            } else {
                choices
                if let error {
                    ErrorNote(message: error)
                }
                Button(action: save) {
                    Text(saving ? String(localized: "Sharing…") : actionTitle).frame(maxWidth: .infinity).shimmer(saving)
                }
                .buttonStyle(.kodosi(.primary, size: .large)).disabled(saving || !changed)
                .accessibilityIdentifier("sharing.save")
                Text("Everyone you share with can type, resize and close this terminal.")
                    .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(20)
        .popoverSheet(width: 360)
        .animation(theme.motion.snappy, value: people)
        .animation(theme.motion.snappy, value: room)
        .onAppear {
            originalPeople = Set(current.sharedWith); people = originalPeople; room = current.missionId
        }
        .onChange(of: deps.accountEpoch) { _, _ in dismiss() }
    }

    @ViewBuilder
    private var choices: some View {
        if !deps.missions.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text("A room").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                FlowRow(spacing: 6) {
                    ForEach(deps.missions) { mission in
                        let selected = room == mission.id
                        Button {
                            room = selected ? nil : mission.id
                            if !selected {
                                people = originalPeople
                            }
                        } label: {
                            HStack(spacing: 7) {
                                RoomSigil(name: mission.name, key: mission.id, size: 18)
                                Text(mission.name).appTextStyle(.footnote).fontWeight(.medium).lineLimit(1)
                                if selected {
                                    Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)).transition(AnyTransition.pop)
                                }
                            }
                            .foregroundStyle(selected ? theme.colors.accentStrong : theme.colors.ink)
                            .padding(.leading, 5).padding(.trailing, 10).frame(height: 28)
                            .background(selected ? theme.colors.accentSoft : theme.colors.ink.opacity(0.07), in: Capsule())
                            .overlay { Capsule().strokeBorder(theme.colors.accent.opacity(selected ? 0.6 : 0), lineWidth: 1) }
                        }
                        .buttonStyle(PressScaleStyle()).disabled(saving)
                        .accessibilityAddTraits(selected ? .isSelected : [])
                    }
                }
            }
        }
        if room == nil, current.missionId == nil {
            VStack(alignment: .leading, spacing: 8) {
                Text("People").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                if deps.friends.isEmpty {
                    Button { dismiss(); deps.workbench.section = .people } label: { Label("Add a friend", systemImage: "person.badge.plus") }
                        .buttonStyle(.kodosi(.tinted))
                } else {
                    ScrollView {
                        FlowRow(spacing: 10) {
                            ForEach(deps.friends) { friend in friendChip(friend) }
                        }
                        .padding(3)
                    }
                    .frame(maxHeight: 170)
                }
            }
            .transition(.opacity)
        }
    }

    private func friendChip(_ friend: FriendEntry) -> some View {
        let name = friend.displayName.isEmpty ? friend.handle : friend.displayName
        let selected = people.contains(friend.userId)
        let blocked = friend.identityChanged && !selected
        return Button {
            if selected {
                people.remove(friend.userId)
            } else {
                people.insert(friend.userId)
            }
        } label: {
            VStack(spacing: 5) {
                PersonAvatar(name: name, key: friend.userId, size: 40)
                    .overlay { Circle().strokeBorder(theme.colors.accent, lineWidth: selected ? 2.5 : 0).padding(-4) }
                    .overlay(alignment: .bottomTrailing) {
                        if selected {
                            Image(systemName: "checkmark").font(.system(size: 8, weight: .heavy)).foregroundStyle(theme.colors.onAccent)
                                .frame(width: 16, height: 16).background(theme.colors.accent, in: Circle())
                                .overlay { Circle().stroke(theme.colors.raised, lineWidth: 2) }
                                .offset(x: 5, y: 5).transition(AnyTransition.pop)
                        } else if friend.identityChanged {
                            Circle().fill(theme.colors.caution).frame(width: 12, height: 12)
                                .overlay { Circle().stroke(theme.colors.raised, lineWidth: 2) }.offset(x: 3, y: 3)
                        }
                    }
                Text(name.split(separator: " ").first.map(String.init) ?? name)
                    .appTextStyle(.caption).foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted).lineLimit(1)
            }
            .frame(width: 60).padding(.top, 4).opacity(blocked ? 0.45 : 1)
        }
        .buttonStyle(PressScaleStyle()).disabled(saving || blocked)
        .help(blocked ? String(localized: "Their identity changed. Trust them in People first.") : name)
        .accessibilityLabel(name).accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func save() {
        saving = true
        error = nil
        Task { @MainActor in
            defer { saving = false }
            do {
                if room != current.missionId {
                    try await deps.mutateSession("session.attachMission", session: session,
                                                 fields: ["missionId": room.map(JSONValue.string) ?? .null])
                } else {
                    try await deps.mutateSession("session.share", session: session, fields: [
                        "userIds": .array(people.sorted().map(JSONValue.string)),
                        "expectedUserIds": .array(originalPeople.sorted().map(JSONValue.string)),
                    ])
                }
                dismiss()
            } catch { self.error = error.localizedDescription }
        }
    }
}

struct FlowRow: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        arrange(width: proposal.width ?? .infinity, subviews: subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        let result = arrange(width: bounds.width, subviews: subviews)
        for (index, origin) in result.origins.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + origin.x, y: bounds.minY + origin.y), anchor: .topLeading, proposal: .unspecified)
        }
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> (size: CGSize, origins: [CGPoint]) {
        var origins: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var widest: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            origins.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            widest = max(widest, x - spacing)
        }
        return (CGSize(width: widest, height: y + rowHeight), origins)
    }
}
