import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: ProjectDetail
    let state: ProjectViewState
    @Bindable var workspace: ProjectWorkspace
    @State private var dragStart: CGFloat?
    @State private var handleHovered = false
    @State private var dropping = false

    private var projectId: String {
        detail.project.id
    }

    var body: some View {
        VStack(spacing: 0) {
            ProjectHeader(detail: detail, state: state, workspace: workspace)
            GeometryReader { geometry in
                let narrow = geometry.size.width < 720
                let width = min(max(workspace.conversationWidth, 300), max(300, min(540, geometry.size.width - 380)))
                ZStack(alignment: .trailing) {
                    if !(narrow && workspace.conversationVisible) {
                        ProjectContentView(projectId: projectId, state: state, workspace: workspace)
                            .overlay {
                                DropSlot(radius: Radius.xl, shown: dropping, label: String(localized: "Share with \(detail.project.name)"))
                                    .padding(6)
                            }
                            .projectDrop(projectId, deps: deps, targeted: $dropping)
                            .animation(theme.motion.snappy, value: dropping)
                            .padding(.trailing, workspace.conversationVisible ? width + 6 : 0)
                            .transaction(value: workspace.conversationVisible) { $0.animation = nil }
                    }
                    if workspace.conversationVisible {
                        ProjectConversationView(projectId: projectId, detail: detail, state: state, workspace: workspace)
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

struct ProjectHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: ProjectDetail
    let state: ProjectViewState
    @Bindable var workspace: ProjectWorkspace
    @State private var name = ""
    @State private var confirmDelete = false
    @State private var confirmLeave = false
    @FocusState private var nameFocused: Bool

    private var projectId: String {
        detail.project.id
    }

    private var isOwner: Bool {
        deps.projects.isOwner(of: projectId)
    }

    private var sessions: [RuntimeSession] {
        deps.projectSessions(projectId)
    }

    var body: some View {
        HStack(spacing: 12) {
            ProjectSigil(key: projectId, size: 30)
            VStack(alignment: .leading, spacing: 1) {
                if workspace.renaming {
                    TextField("Project name", text: $name)
                        .textFieldStyle(.plain).appTextStyle(.headline).focused($nameFocused).frame(maxWidth: 260)
                        .onSubmit { run { try await $0.rename(projectId, to: name) } }
                        .onExitCommand { workspace.renaming = false }
                        .accessibilityIdentifier("project.name.input")
                } else {
                    Text(detail.project.name).appTextStyle(.headline).foregroundStyle(theme.colors.ink).lineLimit(1)
                        .contentTransition(.interpolate)
                }
                ProjectFacts(people: detail.members.count, terminals: sessions.count)
                    .help("Kodosi encrypts everything in this project end to end.")
            }
            .layoutPriority(1)
            Color.clear.frame(minWidth: 8, maxWidth: .infinity).titleBarGap()
            SectionDock(items: dockItems, selection: workspace.shownSection, identifier: "project.section") { workspace.shownSection = $0 }
            Color.clear.frame(minWidth: 8, maxWidth: .infinity).titleBarGap()
            if let failure = state.failure ?? state.readFailure, !workspace.conversationVisible,
               deps.projects.snapshots[projectId] != nil || workspace.shownSection == 0
            {
                NoticeDot(message: failure) { Task { await deps.projects.read(projectId) } } dismiss: {
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
            .buttonStyle(PressScaleStyle()).help("People").accessibilityLabel(Text("Project members"))
            .accessibilityIdentifier("project.people")
            .kodosiPopover(isPresented: $workspace.peopleVisible, arrowEdge: .bottom) {
                ProjectPeoplePopover(detail: detail, state: state, workspace: workspace)
            }
            if workspace.conversationVisible {
                IconButton(title: "Hide conversation", symbol: "bubble.left.and.bubble.right.fill",
                           identifier: "project.conversation.toggle", size: 30, active: true)
                {
                    workspace.conversationVisible = false
                }
                .transition(.opacity)
            } else {
                ConversationKey(projectId: projectId, state: state, workspace: workspace)
                    .transition(theme.motion.reduced ? .opacity : .scale(scale: 0.6, anchor: .trailing).combined(with: .opacity))
            }
            IconButton(title: "Project options", symbol: "ellipsis", identifier: "project.options", size: 30, open: workspace.optionsVisible) {
                workspace.optionsVisible.toggle()
            }
            .kodosiPopover(isPresented: $workspace.optionsVisible, arrowEdge: .bottom, alignment: .trailing) {
                VStack(alignment: .leading, spacing: 2) {
                    if isOwner {
                        PanelMenuRow(Text("Rename"), icon: .symbol("pencil"), identifier: "project.options.rename") {
                            name = detail.project.name; workspace.renaming = true; nameFocused = true
                        }
                    }
                    PanelMenuRow(Text("Refresh"), icon: .symbol("arrow.clockwise"), identifier: "project.options.refresh") {
                        Task { await deps.projects.read(projectId) }
                    }
                    PanelMenuDivider()
                    if isOwner {
                        PanelMenuRow(Text("Delete project…"), icon: .symbol("trash"), identifier: "project.options.delete", destructive: true) {
                            confirmDelete = true
                        }
                    } else {
                        PanelMenuRow(Text("Leave project…"), icon: .symbol("rectangle.portrait.and.arrow.right"),
                                     identifier: "project.options.leave", destructive: true)
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
        .confirmationDialog("Delete \(detail.project.name)?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete project", role: .destructive) { end { try await $0.delete(projectId) } }
        } message: { Text("Its conversation and sharing end. Terminals keep running.") }
        .confirmationDialog("Leave \(detail.project.name)?", isPresented: $confirmLeave, titleVisibility: .visible) {
            Button("Leave project", role: .destructive) { end { try await $0.leave(projectId) } }
        } message: { Text("You lose its terminals and conversation until someone invites you again.") }
    }

    private var dockItems: [DockItem] {
        [
            DockItem(id: 0, title: String(localized: "Terminals"), symbol: "apple.terminal", count: sessions.count),
            .projectTasks(id: 1, tasks: deps.projects.snapshots[projectId]?.tasks ?? [], fresh: state.fresh.contains(.tasks)),
            DockItem(id: 2, title: String(localized: "Repositories"), symbol: "shippingbox",
                     count: deps.projects.snapshots[projectId]?.repositories.count ?? 0, fresh: state.fresh.contains(.repositories)),
        ]
    }

    private func run(_ work: @escaping (ProjectStore) async throws -> Void) {
        let projects = deps.projects
        Task { @MainActor in
            do {
                try await work(projects)
                workspace.renaming = false
            } catch { state.failure = error.localizedDescription }
        }
    }

    private func end(_ work: @escaping (ProjectStore) async throws -> Void) {
        run { projects in
            try await work(projects)
            deps.workbench.selectedProjectId = nil
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
        .buttonStyle(PressScaleStyle()).help("Something did not work").accessibilityLabel(Text("Project action failed"))
        .accessibilityIdentifier("project.failure")
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

struct ProjectPeoplePopover: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let detail: ProjectDetail
    let state: ProjectViewState
    let workspace: ProjectWorkspace
    @State private var invited: Set<String> = []

    private var isOwner: Bool {
        deps.projects.isOwner(of: detail.project.id)
    }

    private var candidates: [FriendEntry] {
        deps.projects.invitable(deps.people.friends, to: detail.project.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text("In this project").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint).padding(.leading, 8)
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
                                .accessibilityIdentifier("project.invite.\(friend.handle)")
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

    private func memberRow(_ member: ProjectMemberEntry) -> some View {
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
                    Button("Remove from project", role: .destructive) { remove(member) }
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
                try await deps.projects.invite(friend.userId, to: detail.project.id)
            } catch {
                invited.remove(friend.userId)
                state.failure = error.localizedDescription
            }
        }
    }

    private func remove(_ member: ProjectMemberEntry) {
        Task { @MainActor in
            do {
                try await deps.projects.remove(member, from: detail.project.id)
            } catch { state.failure = error.localizedDescription }
        }
    }
}

struct ConversationKey: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let projectId: String
    let state: ProjectViewState
    let workspace: ProjectWorkspace

    private var last: ProjectMessage? {
        deps.projects.snapshots[projectId]?.messages.last
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
        .accessibilityIdentifier("project.conversation.toggle")
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
