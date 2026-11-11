import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectsView: View {
    @Environment(AppDependencies.self) private var deps

    var body: some View {
        if !deps.account.isReady {
            SignInView(
                title: "Work in projects",
                message: "A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.",
            )
        } else if let projectId = deps.workbench.selectedProjectId {
            Group {
                if let detail = deps.projects.details[projectId] {
                    ProjectView(detail: detail, state: deps.projects.view(projectId), workspace: deps.projectWorkspace(projectId))
                        .id(projectId)
                        .transition(.opacity)
                } else if let failure = deps.projects.detailFailures[projectId] {
                    ProjectRecovery(message: failure) { Task { await deps.projects.open(projectId) } }
                } else {
                    ProjectSkeleton()
                }
            }
            .showsProject(projectId, in: deps.projects)
        } else {
            ProjectsLobby()
        }
    }
}

struct ProjectsLobby: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                PageHeader(title: "Projects", facts: facts) {
                    Button { deps.workbench.showsNewProject = true } label: { Label("New project", systemImage: "plus") }
                        .buttonStyle(.kodosi(.primary)).accessibilityIdentifier("projects.create.lobby")
                }.arrive()
                if !deps.projects.invitations.isEmpty {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(deps.projects.invitations) { invitation in
                            InvitationCard(invitation: invitation, identifier: "projects.invitation.\(AccessibilityIdentifier.token(invitation.projectId))") {
                                deps.respond(to: invitation, accept: false)
                            } join: {
                                deps.respond(to: invitation, accept: true)
                            }
                        }
                    }.arrive(delay: 0.04)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 250, maximum: 340), spacing: 14)], alignment: .leading, spacing: 14) {
                    ForEach(Array(deps.projects.entries.enumerated()), id: \.element.id) { index, project in
                        ProjectCard(project: project, sessions: deps.projectSessions(project.id), yours: deps.projects.isOwner(of: project.id)) {
                            deps.openProject(project)
                        } detail: {
                            EmptyView()
                        }
                        .accessibilityIdentifier("projects.card.\(AccessibilityIdentifier.token(project.id))")
                        .arrive(delay: 0.04 * Double(min(index, 8) + 1))
                    }
                    Button { deps.workbench.showsNewProject = true } label: {
                        DashedSlot(radius: Radius.xl) {
                            VStack(spacing: 10) {
                                Image(systemName: "plus").font(.system(size: 18, weight: .semibold))
                                Text(deps.projects.entries.isEmpty ? "Make your first project" : "New project").appTextStyle(.subhead)
                            }
                            .foregroundStyle(theme.colors.inkMuted)
                            .frame(maxWidth: .infinity).frame(height: 132)
                        }
                    }
                    .buttonStyle(PressScaleStyle(scale: 0.98)).accessibilityIdentifier("projects.create.slot")
                    .arrive(delay: 0.04 * Double(min(deps.projects.entries.count, 8) + 1))
                }
            }
            .padding(.horizontal, 36).padding(.top, 54).padding(.bottom, 36)
            .frame(maxWidth: 1100, alignment: .leading).frame(maxWidth: .infinity)
        }
    }

    private var facts: String? {
        guard !deps.projects.entries.isEmpty else {
            return String(localized: "A project gathers terminals from any folder and computer, with its tasks and repositories. Invite friends when you want.")
        }
        let shared = deps.sessions.count { $0.projectId != nil && $0.status != .closing }
        return "\(Counted.projects(deps.projects.entries.count)) · " + String(localized: "\(shared) shared terminals")
    }
}

struct NewProjectSheet: View {
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
            ProjectSigil(key: name.isEmpty ? "new" : name, size: 64)
                .animation(theme.motion.spring, value: name)
            VStack(spacing: 6) {
                Text("Name your project").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Text("Invite friends after. Your terminals in the project go to your friends.")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
            }
            TextField("Launch week", text: $name)
                .textFieldStyle(WellTextFieldStyle(radius: Radius.lg)).focused($focused)
                .onSubmit(create).accessibilityIdentifier("projects.create.name")
            if let error {
                ErrorNote(message: error)
            }
            HStack(spacing: 8) {
                Button("Cancel") { deps.workbench.showsNewProject = false }.keyboardShortcut(.cancelAction).buttonStyle(.kodosi(.ghost, size: .large))
                Button(action: create) {
                    Text(creating ? "Making…" : "Make project").frame(maxWidth: .infinity).shimmer(creating)
                }
                .buttonStyle(.kodosi(.primary, size: .large)).disabled(creating || !valid)
                .accessibilityIdentifier("projects.create.confirm")
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
                let id = try await deps.projects.create(name: name, inviting: [])
                deps.workbench.showsNewProject = false
                name = ""
                deps.workbench.section = .projects
                deps.openProject(id: id)
            } catch { self.error = error.localizedDescription }
        }
    }
}
