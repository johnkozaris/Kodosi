import KodosiBridge
import KodosiDesign
import SwiftUI

struct SidebarSectionHeader: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let title: LocalizedStringKey
    let addTitle: LocalizedStringKey
    let identifier: String
    let open: () -> Void
    let add: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            Button(action: open) {
                HStack {
                    Text(title).appTextStyle(.caption)
                        .foregroundStyle(hovered ? theme.colors.ink : theme.colors.inkFaint)
                    Spacer(minLength: 0)
                }
                .padding(.leading, 10).frame(height: 26).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            IconButton(title: addTitle, symbol: "plus", identifier: identifier, size: 22, action: add)
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
    var tint: Color?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: symbol).font(.system(size: 13, weight: .medium))
                    .foregroundStyle(tint ?? (selected ? theme.colors.accentStrong : theme.colors.inkMuted)).frame(width: 22)
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
    @State private var dropping = false

    private var live: [RuntimeSession] {
        deps.roomSessions(mission.id)
    }

    var body: some View {
        Button { deps.workbench.section = .missions; deps.openMission(mission) } label: {
            HStack(spacing: 9) {
                RoomSigil(key: mission.id, size: 22).fold(.room(mission.id), at: .row)
                Text(mission.name).appTextStyle(.body).fontWeight(selected || mission.unread ? .medium : .regular)
                    .foregroundStyle(theme.colors.ink).lineLimit(1)
                Spacer(minLength: 4)
                if mission.unread {
                    NewDot().padding(.trailing, 4).transition(AnyTransition.pop)
                } else if live.contains(where: \.needsUser) {
                    StatusSign(form: .hand).transition(AnyTransition.pop)
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
            .overlay { DropSlot(radius: Radius.md, shown: dropping) }
        }
        .buttonStyle(.plain)
        .roomDrop(mission.id, deps: deps, targeted: $dropping)
        .animation(theme.motion.snappy, value: dropping)
        .accessibilityIdentifier("missions.mission.\(AccessibilityIdentifier.token(mission.id))")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityValue(mission.unread ? Text("New messages") : Text(verbatim: ""))
        .animation(theme.motion.snappy, value: mission.unread)
    }
}

struct SidebarInvitationRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let invitation: MissionInvitationEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 9) {
                RoomSigil(key: invitation.missionId, size: 22)
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
        .background(theme.colors.accentSoft.mix(with: theme.colors.ground, by: 0.45),
                    in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
                .strokeBorder(theme.colors.accent.mix(with: theme.colors.ground, by: 0.55), lineWidth: 1)
        }
        .padding(.vertical, 2)
    }
}

struct SidebarFolderHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    @State private var branching = false
    @State private var choosingOffline = false
    let group: SessionFolderGroup
    let collapsed: Bool
    let toggle: () -> Void

    private var branches: Bool {
        group.computer == nil && group.sessions.contains { $0.repository == true }
    }

    var body: some View {
        HStack(spacing: 2) {
            Button(action: toggle) {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right").font(.system(size: 8, weight: .bold))
                        .rotationEffect(.degrees(collapsed ? 0 : 90))
                        .foregroundStyle(theme.colors.inkFaint).frame(width: 10)
                    Image(systemName: "folder.fill")
                        .font(.system(size: 10, weight: .medium)).foregroundStyle(theme.colors.inkFaint).frame(width: 13)
                    Text(group.name)
                        .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted).lineLimit(1).layoutPriority(1)
                    if collapsed {
                        Text(group.sessions.count, format: .number).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                        if let sign = group.sessions.first(where: \.needsUser)?.sign(unseen: false) {
                            StatusSign(form: sign, size: 10).transition(AnyTransition.pop)
                        }
                    }
                    Spacer(minLength: 4)
                }
                .padding(.leading, 8).frame(height: 24).contentShape(Rectangle())
            }
            .buttonStyle(.plain).help(group.directory ?? group.name)
            .accessibilityValue(Text(collapsed ? "Collapsed" : "Expanded"))
            .accessibilityIdentifier("sidebar.folder.\(AccessibilityIdentifier.token(group.id))")
            .contextMenu {
                if group.canStartTerminal, let directory = group.directory {
                    Button("New terminal here") { deps.openTerminal(in: directory, computer: group.computerId) }
                    if branches {
                        Button("New terminal on a new branch…") { branching = true }
                    }
                }
                if group.away {
                    Button("Close terminals", role: .destructive) { choosingOffline = true }
                }
            }
            if group.away {
                IconButton(title: "Close terminals", symbol: "xmark",
                           identifier: "sidebar.folder.\(AccessibilityIdentifier.token(group.id)).close", size: 20, destructive: true)
                {
                    choosingOffline = true
                }
                .opacity(hovered || choosingOffline ? 1 : 0)
            }
            if group.canStartTerminal, let directory = group.directory {
                if branches {
                    IconButton(title: "New terminal on a new branch", symbol: "arrow.triangle.branch",
                               identifier: "sidebar.folder.\(AccessibilityIdentifier.token(group.id)).branch", size: 20, open: branching)
                    {
                        branching = true
                    }
                    .opacity(hovered || branching ? 1 : 0)
                    .kodosiPopover(isPresented: $branching, arrowEdge: .trailing) { NewBranchPopover(directory: directory) }
                }
                IconButton(title: "New terminal here", symbol: "plus",
                           identifier: "sidebar.folder.\(AccessibilityIdentifier.token(group.id)).new", size: 20)
                {
                    deps.openTerminal(in: directory, computer: group.computerId)
                }
                .opacity(hovered ? 1 : 0)
            }
        }
        .padding(.top, 6)
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .offlineChoice($choosingOffline, for: group.sessions)
    }
}

struct SidebarPinnedHeader: View {
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 6) {
            Color.clear.frame(width: 10)
            Image(systemName: "pin.fill").font(.system(size: 9, weight: .semibold))
                .foregroundStyle(theme.colors.inkFaint).frame(width: 13)
            Text("Pinned").appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            Spacer(minLength: 0)
        }
        .padding(.leading, 8).frame(height: 24).padding(.top, 6)
        .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
        .accessibilityIdentifier("sidebar.pinned")
    }
}

struct SidebarComputerHeader: View {
    @Environment(\.theme) private var theme
    let group: SessionComputerGroup

    var body: some View {
        HStack(spacing: 5) {
            if let owner = group.owner {
                PersonAvatar(name: owner.name, key: owner.userId, size: 13)
            } else {
                DeviceGlyph(label: group.title).font(.system(size: 9, weight: .semibold)).frame(width: 13)
            }
            Text(group.title).appTextStyle(.caption).lineLimit(1)
            if group.away {
                Image(systemName: "wifi.slash").font(.system(size: 8, weight: .bold))
            }
            Spacer(minLength: 0)
        }
        .foregroundStyle(theme.colors.inkFaint)
        .padding(.leading, 10).frame(height: 20).padding(.top, 10)
        .help(help)
        .accessibilityElement(children: .ignore).accessibilityLabel(Text(help)).accessibilityAddTraits(.isHeader)
        .accessibilityIdentifier("sidebar.computer.\(AccessibilityIdentifier.token(group.id))")
    }

    private var help: String {
        guard let computer = group.computer else { return group.title }
        let place = group.owner.map { String(localized: "\($0.name)’s \(computer)") } ?? computer
        return group.away ? String(localized: "\(place) is offline") : place
    }
}

struct SidebarTerminalRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let selected: Bool
    let pill: Namespace.ID
    var showsOwner = true
    @State private var hovered = false
    @State private var confirmingClose = false
    @State private var choosingOffline = false
    @State private var renaming = false
    @State private var draft = ""
    @FocusState private var nameFocused: Bool

    private var staged: Bool {
        deps.workbench.stagedSessionIds.contains(session.id)
    }

    private var troubled: Bool {
        session.isTroubled
    }

    private var sharing: String? {
        if session.missionId != nil {
            return session.missionName.map { String(localized: "Shared with \($0)") } ?? String(localized: "Shared with a room")
        }
        return session.isOwner && !session.sharedWith.isEmpty ? String(localized: "Shared") : nil
    }

    var body: some View {
        HStack(spacing: 4) {
            Button { deps.activateSession(session.id) } label: { label }
                .buttonStyle(.plain)
                .accessibilityIdentifier(AccessibilityIdentifier.sidebarSession(session.id))
                .accessibilityAddTraits(selected ? .isSelected : [])
                .accessibilityValue(Text(sharing ?? ""))
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
        .animation(theme.motion.snappy, value: deps.sign(of: session))
        .help(session.isConnected && !troubled ? (staged ? String(localized: "Open") : String(localized: "Minimized")) : session.statusLabel)
        .contextMenu { menu }
        .terminalDrag(session)
        .confirmationDialog("Close \(session.name)?", isPresented: $confirmingClose, titleVisibility: .visible) {
            Button("Close", role: .destructive) { Task { await deps.close(session) } }
            Button("Cancel", role: .cancel) {}
        } message: { Text("Its programs stop.") }
        .offlineChoice($choosingOffline, for: [session])
    }

    private func askToClose() {
        if session.hostOffline {
            choosingOffline = true
        } else {
            confirmingClose = true
        }
    }

    private var label: some View {
        HStack(spacing: 9) {
            AgentMark(session: session, size: 22, activity: deps.activity(of: session))
                .overlay(alignment: .bottomTrailing) {
                    if let sharing {
                        ShareBadge(room: session.missionId, ring: selected ? theme.colors.raised : theme.colors.ground)
                            .offset(x: 4, y: 4)
                            .help(sharing)
                    }
                }
                .fold(.terminal(session.id), at: .row)
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
                    Text(session.name).appTextStyle(.body).fontWeight(selected ? .medium : .regular)
                        .foregroundStyle(staged || selected ? theme.colors.ink : theme.colors.inkMuted).lineLimit(1)
                        .contentTransition(.interpolate)
                }
                let active = session.activity != nil || session.progress != nil
                if session.status == .offline {
                    Text(session.statusLabel).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                        .lineLimit(1)
                } else if session.place != nil || active {
                    HStack(spacing: 4) {
                        if let place = session.place {
                            SessionPlace(place: place, glyphFont: .system(size: 9, weight: .semibold)).layoutPriority(1)
                                .help(session.placeLabel ?? "")
                        }
                        if active {
                            if session.place != nil {
                                Text(verbatim: "·")
                            }
                            SessionActivityText(session: session)
                        }
                    }
                    .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
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
                    askToClose()
                }.disabled(!session.canClose)
            }
            .transition(.opacity)
        } else {
            HStack(spacing: 5) {
                let viewers = deps.viewers(of: session)
                if !viewers.isEmpty {
                    AvatarStack(people: viewers, size: 15, limit: 2, ring: selected ? theme.colors.raised : theme.colors.ground)
                }
                if let sign = deps.sign(of: session) {
                    StatusSign(form: sign).transition(AnyTransition.pop)
                } else if troubled {
                    Circle().fill(theme.colors.caution).frame(width: 6, height: 6)
                } else if showsOwner, !session.isOwner, let owner = session.ownerName {
                    PersonAvatar(name: owner, key: session.ownerUserId, size: 15).help(String(localized: "Shared by \(owner)"))
                }
            }
            .padding(.trailing, 6)
        }
    }

    @ViewBuilder
    private var menu: some View {
        Button("Open") { deps.activateSession(session.id) }
        Button(deps.workbench.isPinned(session.id) ? "Unpin" : "Pin") {
            withAnimation(theme.motion.spring) { deps.workbench.togglePin(session.id) }
        }
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
        Button("Close terminal", role: .destructive) { askToClose() }.disabled(!session.canClose)
    }
}
