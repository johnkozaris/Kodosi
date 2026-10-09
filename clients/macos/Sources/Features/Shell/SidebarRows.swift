import SwiftUI

struct SidebarSectionHeader: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let title: LocalizedStringKey
    let addTitle: LocalizedStringKey
    let identifier: String
    let selected: Bool
    let pill: Namespace.ID
    let open: () -> Void
    let add: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            Button(action: open) {
                HStack {
                    Text(title).appTextStyle(.caption)
                        .foregroundStyle(selected || hovered ? theme.colors.ink : theme.colors.inkFaint)
                    Spacer(minLength: 0)
                }
                .padding(.leading, 10).frame(height: 26).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            IconButton(title: addTitle, symbol: "plus", identifier: identifier, size: 22, action: add)
        }
        .background {
            if selected {
                SelectionPill(namespace: pill, radius: Radius.sm)
            }
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
    }
}

struct SidebarNavRow: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    let symbol: String
    let identifier: String
    let badge: Int
    let selected: Bool
    let pill: Namespace.ID
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: symbol).font(.system(size: 13, weight: .medium))
                    .foregroundStyle(selected ? theme.colors.accentStrong : theme.colors.inkMuted).frame(width: 22)
                Text(title).appTextStyle(.body).fontWeight(selected ? .medium : .regular).foregroundStyle(theme.colors.ink)
                Spacer(minLength: 0)
                if badge > 0 {
                    CountBadge(count: badge).transition(AnyTransition.pop)
                }
            }
            .padding(.horizontal, 8).frame(height: 32)
            .background {
                if selected {
                    SelectionPill(namespace: pill)
                }
            }
            .hoverRow(selected: selected)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(identifier)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

struct SidebarRoomRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let mission: MissionEntry
    let selected: Bool
    let pill: Namespace.ID

    private var unread: Int {
        deps.roomViews[mission.id]?.unread ?? 0
    }

    private var live: [RuntimeSession] {
        deps.sessions.filter { $0.missionId == mission.id }
    }

    var body: some View {
        Button { deps.workbench.section = .missions; deps.openMission(mission) } label: {
            HStack(spacing: 9) {
                RoomSigil(name: mission.name, key: mission.id, size: 22)
                Text(mission.name).appTextStyle(.body).fontWeight(selected || unread > 0 ? .medium : .regular)
                    .foregroundStyle(theme.colors.ink).lineLimit(1)
                Spacer(minLength: 4)
                if unread > 0 {
                    CountBadge(count: unread).transition(AnyTransition.pop)
                } else if live.contains(where: \.isWorking) {
                    BreathingDot(color: theme.colors.glowOrange, size: 6).padding(.trailing, 4)
                }
            }
            .padding(.horizontal, 8).frame(height: 34)
            .background {
                if selected {
                    SelectionPill(namespace: pill)
                }
            }
            .hoverRow(selected: selected)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("missions.mission.\(AccessibilityIdentifier.token(mission.id))")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .animation(theme.motion.snappy, value: unread)
    }
}

struct SidebarInvitationRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let invitation: MissionInvitationEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 9) {
                RoomSigil(name: invitation.missionName, key: invitation.missionId, size: 22)
                VStack(alignment: .leading, spacing: 1) {
                    Text(invitation.missionName).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(1)
                    Text("\(invitation.inviterName) invited you").appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                }
            }
            HStack(spacing: 6) {
                Button("Join") { deps.respond(to: invitation, accept: true) }
                    .buttonStyle(.kodosi(.primary, size: .small))
                    .accessibilityIdentifier("missions.invitation.\(AccessibilityIdentifier.token(invitation.id)).join")
                Button("Not now") { deps.respond(to: invitation, accept: false) }
                    .buttonStyle(.kodosi(.ghost, size: .small))
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.colors.accentSoft.opacity(0.55), in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).strokeBorder(theme.colors.accent.opacity(0.45), lineWidth: 1) }
        .padding(.vertical, 2)
    }
}

struct SidebarFolderHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let group: SessionFolderGroup
    let collapsed: Bool
    let toggle: () -> Void

    var body: some View {
        HStack(spacing: 2) {
            Button(action: toggle) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right").font(.system(size: 8, weight: .bold))
                        .rotationEffect(.degrees(collapsed ? 0 : 90))
                        .foregroundStyle(theme.colors.inkFaint).frame(width: 10)
                    if let host = group.host {
                        DeviceGlyph(label: host).font(.system(size: 10, weight: .medium)).foregroundStyle(theme.colors.inkFaint)
                    }
                    Text(group.label)
                        .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                    if collapsed {
                        Text(group.sessions.count, format: .number).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                    }
                    Spacer(minLength: 0)
                }
                .padding(.leading, 8).frame(height: 24).contentShape(Rectangle())
            }
            .buttonStyle(.plain).help(group.directory ?? group.name)
            .accessibilityValue(Text(collapsed ? "Collapsed" : "Expanded"))
            if group.host == nil, let directory = group.directory {
                IconButton(title: "New terminal here", symbol: "plus",
                           identifier: "sidebar.folder.\(AccessibilityIdentifier.token(group.id)).new", size: 20)
                {
                    deps.newSession(directory: directory)
                }
                .opacity(hovered ? 1 : 0)
            }
        }
        .padding(.top, 6)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
    }
}

struct SidebarTerminalRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let selected: Bool
    let pill: Namespace.ID
    @State private var hovered = false
    @State private var confirmingClose = false
    @State private var renaming = false
    @State private var draft = ""
    @FocusState private var nameFocused: Bool

    private var staged: Bool {
        deps.workbench.stagedSessionIds.contains(session.id)
    }

    private var needsAttention: Bool {
        deps.attention.contains(session.id)
    }

    private var troubled: Bool {
        session.isTroubled
    }

    var body: some View {
        HStack(spacing: 4) {
            Button { deps.activateSession(session.id) } label: { label }
                .buttonStyle(.plain)
                .accessibilityIdentifier(AccessibilityIdentifier.sidebarSession(session.id))
                .accessibilityAddTraits(selected ? .isSelected : [])
            trailing
        }
        .padding(.leading, 8).padding(.trailing, 4).frame(height: 38)
        .background {
            if selected {
                SelectionPill(namespace: pill)
            }
        }
        .hoverRow(selected: selected)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: needsAttention)
        .help(session.isConnected ? (staged ? String(localized: "Open") : String(localized: "Minimized")) : session.statusLabel)
        .contextMenu { menu }
        .confirmationDialog("Close \(session.name)?", isPresented: $confirmingClose, titleVisibility: .visible) {
            Button("Close", role: .destructive) { deps.close(session) }
            Button("Cancel", role: .cancel) {}
        } message: { Text("Its programs stop.") }
    }

    private var label: some View {
        HStack(spacing: 9) {
            AgentMark(kind: session.agent, size: 22, activity: deps.activity(of: session))
            VStack(alignment: .leading, spacing: 1) {
                if renaming {
                    TextField("Name", text: $draft)
                        .textFieldStyle(.plain).appTextStyle(.body).focused($nameFocused)
                        .onSubmit { deps.rename(session, to: draft); renaming = false }
                        .onExitCommand { renaming = false }
                        .onChange(of: nameFocused) { _, focused in
                            if !focused {
                                renaming = false
                            }
                        }
                        .accessibilityIdentifier("\(AccessibilityIdentifier.sidebarSession(session.id)).name")
                } else {
                    Text(session.name).appTextStyle(.body).fontWeight(selected || needsAttention ? .medium : .regular)
                        .foregroundStyle(staged || selected ? theme.colors.ink : theme.colors.inkMuted).lineLimit(1)
                        .contentTransition(.interpolate)
                }
                if let activity = session.activity {
                    Text(activity).appTextStyle(.caption).fontWeight(.regular)
                        .foregroundStyle(theme.colors.inkFaint).lineLimit(1)
                        .shimmer(session.isWorking)
                }
            }
            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private var trailing: some View {
        if hovered, !renaming {
            HStack(spacing: 0) {
                if staged {
                    IconButton(title: "Minimize", symbol: "minus",
                               identifier: "\(AccessibilityIdentifier.sidebarSession(session.id)).minimize", size: 22)
                    {
                        deps.dismissSession(session.id)
                    }
                }
                IconButton(title: "Close terminal", symbol: "xmark",
                           identifier: "\(AccessibilityIdentifier.sidebarSession(session.id)).close", size: 22, destructive: true)
                {
                    confirmingClose = true
                }.disabled(!session.canControl)
            }
            .transition(.opacity)
        } else {
            HStack(spacing: 5) {
                let viewers = deps.viewers(of: session)
                if !viewers.isEmpty {
                    AvatarStack(people: viewers, size: 15, limit: 2, ring: selected ? theme.colors.raised : theme.colors.ground)
                }
                if needsAttention {
                    BreathingDot(color: theme.colors.accent, size: 7).transition(AnyTransition.pop)
                        .accessibilityLabel(Text("Needs you"))
                } else if troubled {
                    Circle().fill(theme.colors.caution).frame(width: 6, height: 6)
                } else if !session.isOwner, let owner = session.ownerName {
                    PersonAvatar(name: owner, key: session.ownerUserId, size: 15).help(String(localized: "Shared by \(owner)"))
                } else if let room = session.missionId {
                    RoomSigil(name: session.missionName ?? "", key: room, size: 14)
                        .help(session.missionName.map { String(localized: "Shared with \($0)") } ?? "")
                } else if !session.sharedWith.isEmpty {
                    Image(systemName: "person.2.fill").font(.system(size: 8)).foregroundStyle(theme.colors.inkFaint)
                        .help("Shared")
                }
            }
            .padding(.trailing, 6)
        }
    }

    @ViewBuilder
    private var menu: some View {
        Button("Open") { deps.activateSession(session.id) }
        if session.isOwner {
            Button("Rename…") { draft = session.name; renaming = true; nameFocused = true }
        }
        if session.isOwner, session.kind == .local {
            Button("Share…") { deps.activateSession(session.id); deps.workbench.sharingSessionId = session.id }
        }
        Button("Details") { deps.workbench.detailsSessionId = session.id }
        if staged {
            Button("Minimize") { deps.dismissSession(session.id) }
        }
        Divider()
        Button("Close terminal", role: .destructive) { confirmingClose = true }.disabled(!session.canControl)
    }
}
