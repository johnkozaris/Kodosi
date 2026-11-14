import KodosiBridge
import KodosiDesign
import SwiftUI

struct ProjectMessageRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let projectId: String
    let message: ProjectMessage
    let previous: ProjectMessage?
    let targets: [ProjectMention]
    let state: ProjectViewState
    let workspace: ProjectWorkspace
    @State private var hovered = false
    @State private var madeTask = false

    private var date: Date? {
        ProjectDates.parse(message.createdAt)
    }

    private var isMine: Bool {
        deps.projects.isOwn(message)
    }

    private var author: ProjectMemberEntry? {
        deps.projects.otherAuthor(of: message, in: projectId)
    }

    private var terminal: RuntimeSession? {
        message.terminalId.flatMap(deps.session)
    }

    private var startsGroup: Bool {
        ProjectDates.startsGroup(message, after: previous)
    }

    var body: some View {
        let mentioned = !isMine && ProjectMentions.mentionsSelf(message.text, targets: targets)
        VStack(spacing: 0) {
            if ProjectDates.startsDay(message, after: previous), let date {
                ProjectDayDivider(date: date)
            }
            Group {
                if isMine {
                    mine
                } else {
                    theirs
                }
            }
            .projectMention(mentioned)
            .padding(.top, startsGroup ? 12 : 3)
            .overlay(alignment: .topTrailing) {
                if hovered {
                    actions.offset(y: startsGroup ? 2 : -10).transition(.opacity.combined(with: .scale(scale: 0.94, anchor: .trailing)))
                }
            }
        }
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .transition(AnyTransition.rise)
        .accessibilityIdentifier("project.message.\(AccessibilityIdentifier.token(message.id))")
    }

    private var mine: some View {
        HStack(alignment: .bottom, spacing: 6) {
            Spacer(minLength: 44)
            if hovered, let date {
                ProjectMessageTime(date: date).transition(.opacity)
            }
            ProjectMessageBody(text: message.text, targets: targets).projectBubble()
        }
    }

    private var theirs: some View {
        HStack(alignment: .top, spacing: 10) {
            ZStack(alignment: .top) {
                if startsGroup {
                    ProjectMessageFace(message: message, terminal: terminal)
                } else if hovered, let date {
                    ProjectMessageTime(date: date).lineLimit(1).fixedSize().padding(.top, 3)
                }
            }
            .frame(width: 28)
            VStack(alignment: .leading, spacing: 3) {
                if startsGroup {
                    header
                }
                ProjectMessageBody(text: message.text, targets: targets).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var header: some View {
        HStack(spacing: 6) {
            ProjectMessageAuthor(message: message, handle: deps.projects.member(message.authorId, in: projectId)?.handle)
            if let terminal {
                Button {
                    workspace.shownSection = 0
                    workspace.selectedTerminal = terminal.id
                    deps.activateSession(terminal.id, inProject: projectId)
                } label: {
                    ProjectTerminalChip(name: terminal.name)
                }
                .buttonStyle(PressScaleStyle()).help("Open this terminal")
            }
            if let date {
                ProjectMessageTime(date: date)
            }
            Spacer(minLength: 0)
        }
    }

    private var actions: some View {
        HStack(spacing: 0) {
            IconButton(title: "Copy", symbol: "doc.on.doc", identifier: "project.message.copy", size: 24) {
                deps.copyText(message.text)
            }
            IconButton(title: madeTask ? "Added to tasks" : "Make this a task", symbol: madeTask ? "checkmark" : "checklist",
                       identifier: "project.message.task", size: 24, active: madeTask, action: makeTask)
                .disabled(madeTask || workspace.busy)
            if let author {
                let name = author.personName
                Menu {
                    Button("Block…", role: .destructive) { deps.workbench.blocking = BlockTarget(handle: author.handle, name: name) }
                    Button("Report…", role: .destructive) {
                        deps.workbench.reporting = ReportTarget(userId: author.userId, handle: author.handle, name: name)
                    }
                } label: {
                    Image(systemName: "ellipsis").font(.system(size: 11, weight: .semibold)).foregroundStyle(theme.colors.inkMuted)
                        .frame(width: 24, height: 24).contentShape(Circle())
                }
                .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
                .accessibilityLabel(Text("Options for \(name)"))
                .accessibilityIdentifier("project.message.more")
            }
        }
        .padding(2)
        .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
    }

    private func makeTask() {
        Task { @MainActor in
            do {
                try await deps.projects.makeTask(from: message, in: projectId)
                madeTask = true
            } catch { state.failure = error.localizedDescription }
        }
    }
}
