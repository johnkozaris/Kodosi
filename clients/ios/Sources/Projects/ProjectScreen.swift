import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.dismiss) private var dismiss
    let projectId: String
    @State private var width: CGFloat = 0
    @State private var terminal: TerminalRoute?
    @State private var leaving = false

    private var projects: ProjectStore {
        model.projects
    }

    private var wide: Bool {
        sizeClass == .regular && width >= 720
    }

    private var present: Bool {
        projects.project(projectId) != nil && !(projects.departed.contains(projectId) && !leaving)
    }

    var body: some View {
        @Bindable var screen = model.projectScreen(projectId)
        content(screen)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(wide || screen.section != .conversation ? theme.colors.surface : theme.colors.sheet)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .navigationTitle(projects.project(projectId)?.name ?? "")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar(screen) }
            .toolbarVisibility(wide ? .automatic : .hidden, for: .tabBar)
            .safeAreaBar(edge: .top) {
                if present, !wide {
                    dock(sections: [.conversation, .tasks, .terminals], selection: screen.section) { screen.section = $0 }
                        .background(screen.section == .conversation ? theme.colors.sheet : theme.colors.surface)
                }
            }
            .showsProject(projectId, in: projects)
            .sheet(isPresented: $screen.showsInfo, onDismiss: finishLeaving) {
                ProjectInfoSheet(projectId: projectId) {
                    leaving = true
                    screen.showsInfo = false
                }
            }
            .sheet(item: $screen.selectedText) { ProjectTextSheet(text: $0.text) }
            .navigationDestination(item: $terminal) { TerminalScreen(sessionId: $0.sessionId) }
            .environment(\.openURL, OpenURLAction(handler: open))
            .sensoryFeedback(.selection, trigger: screen.section) { _, _ in !wide }
            .sensoryFeedback(.selection, trigger: screen.shownSection) { _, _ in wide }
    }

    @ViewBuilder
    private func content(_ screen: ProjectScreenState) -> some View {
        if leaving {
            Color.clear
        } else if projects.departed.contains(projectId) {
            EmptyState(title: "You are no longer in this project", message: "It ended, or someone removed you.") {
                ProjectSigil(key: projectId, size: 56)
            } actions: {
                Button("Back to projects") { dismiss() }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("projects.departed.back")
            }
        } else if projects.project(projectId) == nil {
            if let failure = projects.detailFailures[projectId] {
                ProjectRecovery(message: failure) { Task { await projects.open(projectId) } }
            } else {
                ProjectSkeleton()
            }
        } else if wide {
            HStack(alignment: .top, spacing: 12) {
                VStack(spacing: 0) {
                    dock(sections: [.terminals, .tasks], selection: screen.shownSection) { screen.shownSection = $0 }
                    section(screen.shownSection == .conversation ? .terminals : screen.shownSection)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                ProjectConversation(projectId: projectId)
                    .frame(width: min(480, max(340, width * 0.4)))
                    .clipShape(RoundedRectangle(cornerRadius: Radius.xl, style: .continuous))
                    .raised(Radius.xl, fill: theme.colors.sheet)
                    .padding(.bottom, 12)
            }
            .padding(.trailing, 12)
        } else {
            section(screen.section)
        }
    }

    private func section(_ section: ProjectSection) -> some View {
        Group {
            switch section {
            case .conversation: ProjectConversation(projectId: projectId)
            case .tasks: ProjectTasks(projectId: projectId)
            case .terminals: ProjectTerminals(projectId: projectId)
            }
        }
        .id(section)
        .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: theme.motion.reduced ? 0 : 6)), removal: .identity))
        .animation(theme.motion.snappy, value: section)
    }

    private func dock(sections: [ProjectSection], selection: ProjectSection, select: @escaping (ProjectSection) -> Void) -> some View {
        let state = projects.view(projectId)
        let terminals = model.terminals.sessions.filter { $0.projectId == projectId }.count
        let items = sections.map { section in
            switch section {
            case .conversation:
                DockItem(id: section.rawValue, title: String(localized: "Conversation"), symbol: "bubble.left.and.bubble.right",
                         count: state.unread)
            case .tasks:
                DockItem.projectTasks(id: section.rawValue, tasks: projects.snapshots[projectId]?.tasks ?? [], fresh: state.fresh.contains(.tasks))
            case .terminals:
                DockItem(id: section.rawValue, title: String(localized: "Terminals"), symbol: "apple.terminal", count: terminals)
            }
        }
        return SectionDock(items: items, selection: selection.rawValue, identifier: "projects.section") { id in
            if let section = ProjectSection(rawValue: id) {
                select(section)
            }
        }
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity)
    }

    @ToolbarContentBuilder
    private func toolbar(_ screen: ProjectScreenState) -> some ToolbarContent {
        if let project = projects.project(projectId) {
            ToolbarItem(placement: .principal) {
                Button { screen.showsInfo = true } label: { ProjectHeader(project: project) }
                    .buttonStyle(PressScaleStyle(scale: 0.97))
                    .accessibilityHint(Text("Shows the people and settings of this project"))
                    .accessibilityIdentifier("projects.header")
            }
            .sharedBackgroundVisibility(.hidden)
            ToolbarItem(placement: .topBarTrailing) {
                Button { screen.showsInfo = true } label: {
                    AvatarStack(people: members, size: 26, limit: 3)
                }
                .buttonStyle(PressScaleStyle())
                .accessibilityLabel(Text("Project members"))
                .accessibilityIdentifier("projects.members")
            }
            .sharedBackgroundVisibility(.hidden)
        }
    }

    private var members: [AvatarStack.Person] {
        (projects.details[projectId]?.members ?? []).map { member in
            AvatarStack.Person(id: member.userId, name: member.personName, isSelf: member.userId == projects.userId)
        }
    }

    private func open(_ url: URL) -> OpenURLAction.Result {
        guard let id = ProjectMentions.terminalId(in: url) else { return .systemAction }
        if model.terminals.session(id) != nil {
            terminal = TerminalRoute(sessionId: id)
        } else {
            model.notices.show(String(localized: "This terminal is not available."))
        }
        return .handled
    }

    private func finishLeaving() {
        if leaving {
            dismiss()
        }
    }
}

private struct ProjectHeader: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let project: ProjectEntry

    var body: some View {
        let people = model.projects.details[project.id]?.members.count
        let terminals = model.terminals.sessions.filter { $0.projectId == project.id }.count
        HStack(spacing: 9) {
            ProjectSigil(key: project.id, size: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(project.name)
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                ProjectFacts(people: people, terminals: terminals)
            }
        }
        .accessibilityElement(children: .combine)
    }
}
