import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openRoute) private var openRoute
    @State private var query = ""
    @State private var showsNewProject = false
    @State private var opened: ProjectRoute?
    @State private var confirming: ProjectEntry?
    @State private var created: String?

    private var projects: ProjectStore {
        model.projects
    }

    private var shown: [ProjectEntry] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else { return projects.entries }
        return projects.entries.filter { $0.name.localizedStandardContains(needle) }
    }

    var body: some View {
        let shown = shown
        List {
            invitationsSection
            projectsSection(shown)
            placeholder
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(theme.colors.surface)
        .readableWidth()
        .overlay {
            if !query.isEmpty, shown.isEmpty {
                NothingMatches()
            }
        }
        .navigationTitle("Projects")
        .pageFacts(facts)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                IconButton(title: "New project", symbol: "plus", identifier: "projects.new", size: 32) { showsNewProject = true }
            }
            .sharedBackgroundVisibility(.hidden)
        }
        .searchable(text: $query, prompt: Text("Find a project"))
        .refreshable { await projects.refreshList() }
        .task(id: projects.entries.map(\.id)) { await projects.prefetchDetails() }
        .animation(theme.motion.spring, value: projects.entries.map(\.id))
        .animation(theme.motion.spring, value: projects.invitations.map(\.id))
        .sheet(isPresented: $showsNewProject, onDismiss: openCreated) {
            ProjectCreateSheet { created = $0 }
        }
        .confirmationDialog(
            confirmationTitle, isPresented: confirmationShown, titleVisibility: .visible, presenting: confirming,
        ) { project in
            if projects.isOwner(of: project.id) {
                Button("Delete project", role: .destructive) { projects.run { try await projects.delete(project.id) } }
                    .accessibilityIdentifier("projects.delete.confirm")
            } else {
                Button("Leave project", role: .destructive) { projects.run { try await projects.leave(project.id) } }
                    .accessibilityIdentifier("projects.leave.confirm")
            }
        } message: { project in
            if projects.isOwner(of: project.id) {
                Text("Its conversation and sharing end. Terminals keep running.")
            } else {
                Text("You lose its terminals and conversation until someone invites you again.")
            }
        }
        .navigationDestination(for: ProjectRoute.self) { ProjectScreen(projectId: $0.projectId) }
        .navigationDestination(for: TerminalRoute.self) { TerminalScreen(sessionId: $0.sessionId) }
        .navigationDestination(item: $opened) { ProjectScreen(projectId: $0.projectId) }
    }

    @ViewBuilder
    private var invitationsSection: some View {
        if query.isEmpty, !projects.invitations.isEmpty {
            ForEach(projects.invitations) { invitation in
                ProjectInvitationRow(invitation: invitation) { opened = ProjectRoute(projectId: $0) }
                    .cardRow(top: 6, bottom: 6)
            }
        }
    }

    @ViewBuilder
    private func projectsSection(_ shown: [ProjectEntry]) -> some View {
        ForEach(shown) { project in
            projectCard(project)
        }
        if query.isEmpty, projects.listed, !projects.entries.isEmpty {
            Button { showsNewProject = true } label: {
                DashedSlot(radius: Radius.xl) {
                    HStack(spacing: 10) {
                        Image(systemName: "plus").font(.system(size: 16, weight: .semibold))
                        Text("New project").appTextStyle(.subhead)
                    }
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity, minHeight: 64)
                }
            }
            .buttonStyle(PressScaleStyle(scale: 0.98))
            .accessibilityIdentifier("projects.slot.new")
            .cardRow(top: 6, bottom: 6)
        }
        if projects.listTruncated {
            Text("Some projects are not shown.")
                .appTextStyle(.caption).fontWeight(.regular)
                .foregroundStyle(theme.colors.inkFaint)
                .cardRow()
        }
    }

    private func projectCard(_ project: ProjectEntry) -> some View {
        let owner = projects.isOwner(of: project.id)
        return ProjectListCard(project: project) { openRoute(ProjectRoute(projectId: project.id)) }
            .cardRow(top: 6, bottom: 6)
            .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                Button(role: .destructive) { confirming = project } label: {
                    if owner {
                        Label("Delete", systemImage: "trash")
                    } else {
                        Label("Leave", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                }
                .tint(theme.colors.danger)
            }
            .contextMenu {
                if project.unread {
                    Button { projects.markRead(project.id) } label: { Label("Mark as read", systemImage: "checkmark.message") }
                }
                Button(role: .destructive) { confirming = project } label: {
                    if owner {
                        Label("Delete project…", systemImage: "trash")
                    } else {
                        Label("Leave project…", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                }
            }
            .accessibilityIdentifier("projects.row.\(AccessibilityIdentifier.token(project.id))")
    }

    @ViewBuilder
    private var placeholder: some View {
        if !projects.listed {
            ProjectListSkeleton()
                .cardRow()
        } else if query.isEmpty, projects.entries.isEmpty, projects.invitations.isEmpty {
            EmptyState(title: "Make your first project", message: "A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.") {
                ProjectSigil(key: "kodosi", size: 64)
            } actions: {
                Button { showsNewProject = true } label: { Label("New project", systemImage: "plus") }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("projects.empty.new")
            }
            .frame(minHeight: 420)
            .cardRow()
        }
    }

    private var facts: String {
        guard !projects.entries.isEmpty else { return "" }
        let shared = model.terminals.sessions.count { $0.projectId != nil && $0.status != .closing }
        return "\(Counted.projects(projects.entries.count)) · " + String(localized: "\(shared) shared terminals")
    }

    private func openCreated() {
        guard let created else { return }
        self.created = nil
        opened = ProjectRoute(projectId: created)
    }

    private var confirmationShown: Binding<Bool> {
        Binding(get: { confirming != nil }, set: {
            if !$0 {
                confirming = nil
            }
        })
    }

    private var confirmationTitle: String {
        guard let confirming else { return "" }
        return projects.isOwner(of: confirming.id)
            ? String(localized: "Delete \(confirming.name)?")
            : String(localized: "Leave \(confirming.name)?")
    }
}

private struct ProjectListSkeleton: View {
    var body: some View {
        VStack(spacing: 12) {
            ForEach(0 ..< 3, id: \.self) { index in
                VStack(alignment: .leading, spacing: 10) {
                    SkeletonBlock(width: 44, height: 44, radius: 13)
                    SkeletonBlock(width: 120, height: 12)
                    SkeletonBlock(width: index.isMultiple(of: 2) ? 150 : 210, height: 10)
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .raised(Radius.xl)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Loading"))
    }
}
