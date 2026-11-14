import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct ProjectMessageRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL
    let projectId: String
    let message: ProjectMessage
    let previous: ProjectMessage?
    let next: ProjectMessage?
    let targets: [ProjectMention]
    @State private var madeTask = 0

    private var date: Date? {
        ProjectDates.parse(message.createdAt)
    }

    private var isMine: Bool {
        model.projects.isOwn(message)
    }

    private var author: ProjectMemberEntry? {
        isMine ? nil : model.projects.member(message.authorId, in: projectId)
    }

    private var terminal: RuntimeSession? {
        message.terminalId.flatMap { model.terminals.session($0) }
    }

    private var startsGroup: Bool {
        ProjectDates.startsGroup(message, after: previous)
    }

    private var endsGroup: Bool {
        next.map { ProjectDates.startsGroup($0, after: message) } ?? true
    }

    var body: some View {
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
            .padding(.top, startsGroup ? 12 : 3)
            .accessibilityAction(named: Text("Make a task"), makeTask)
        }
        .sensoryFeedback(.success, trigger: madeTask)
        .accessibilityIdentifier("projects.message.\(AccessibilityIdentifier.token(message.id))")
    }

    private var mine: some View {
        HStack(alignment: .bottom, spacing: 0) {
            Spacer(minLength: 56)
            VStack(alignment: .trailing, spacing: 3) {
                ProjectMessageBody(text: message.text, targets: targets)
                    .projectBubble()
                    .contentShape(.contextMenuPreview, ProjectBubble())
                    .contextMenu { actions }
                if endsGroup, let date {
                    ProjectMessageTime(date: date).padding(.trailing, 4)
                }
            }
        }
    }

    private var theirs: some View {
        HStack(alignment: .top, spacing: 10) {
            avatar
                .frame(width: 28, height: startsGroup ? 28 : 1, alignment: .top)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                if startsGroup {
                    header
                }
                ProjectMessageBody(text: message.text, targets: targets).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .projectMention(ProjectMentions.mentionsSelf(message.text, targets: targets))
        .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .contextMenu { actions }
    }

    @ViewBuilder
    private var avatar: some View {
        if startsGroup {
            ProjectMessageFace(message: message, terminal: terminal)
        } else {
            Color.clear
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                ProjectMessageAuthor(message: message, handle: model.projects.member(message.authorId, in: projectId)?.handle)
                if let date {
                    ProjectMessageTime(date: date)
                }
            }
            if let terminal {
                NavigationLink(value: TerminalRoute(sessionId: terminal.id)) {
                    ProjectTerminalChip(name: terminal.name, sign: model.terminals.sign(of: terminal))
                        .touchTarget(Capsule())
                }
                .buttonStyle(PressScaleStyle())
                .accessibilityLabel(Text("Open \(terminal.name)"))
                .accessibilityIdentifier("projects.message.terminal.\(AccessibilityIdentifier.token(message.id))")
            }
        }
    }

    @ViewBuilder
    private var actions: some View {
        Button { UIPasteboard.general.string = message.text } label: {
            Label("Copy", systemImage: "doc.on.doc")
        }
        Button {
            model.projectScreen(projectId).selectedText = ProjectTextSelection(id: message.id, text: message.text)
        } label: {
            Label("Select text", systemImage: "selection.pin.in.out")
        }
        Button(action: makeTask) {
            Label("Make a task", systemImage: "checklist")
        }
        if let terminal, let link = ProjectMentions.terminalLink(terminal.id) {
            Button { openURL(link) } label: {
                Label("Open \(terminal.name)", systemImage: "apple.terminal")
            }
        }
        if let author {
            Button(role: .destructive) { model.projectScreen(projectId).memberAction.block(author) } label: {
                Label("Block…", systemImage: "hand.raised")
            }
            Button(role: .destructive) { model.projectScreen(projectId).memberAction.report(author) } label: {
                Label("Report…", systemImage: "flag")
            }
        }
    }

    private func makeTask() {
        let projects = model.projects
        Task {
            do {
                try await projects.makeTask(from: message, in: projectId)
                madeTask += 1
            } catch {
                projects.report(error)
            }
        }
    }
}
