import KodosiBridge
import KodosiDesign
import SwiftUI

struct RoomMessageRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    let message: RoomMessage
    let previous: RoomMessage?
    let targets: [RoomMention]
    let state: RoomViewState
    let workspace: RoomWorkspace
    @State private var hovered = false
    @State private var madeTask = false

    private var date: Date? {
        RoomDates.parse(message.createdAt)
    }

    private var isMine: Bool {
        deps.rooms.isOwn(message)
    }

    private var terminal: RuntimeSession? {
        message.terminalId.flatMap(deps.session)
    }

    private var startsGroup: Bool {
        RoomDates.startsGroup(message, after: previous)
    }

    var body: some View {
        let mentioned = !isMine && RoomMentions.mentionsSelf(message.text, targets: targets)
        VStack(spacing: 0) {
            if RoomDates.startsDay(message, after: previous), let date {
                RoomDayDivider(date: date)
            }
            Group {
                if isMine {
                    mine
                } else {
                    theirs
                }
            }
            .roomMention(mentioned)
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
        .accessibilityIdentifier("room.message.\(AccessibilityIdentifier.token(message.id))")
    }

    private var mine: some View {
        HStack(alignment: .bottom, spacing: 6) {
            Spacer(minLength: 44)
            if hovered, let date {
                RoomMessageTime(date: date).transition(.opacity)
            }
            RoomMessageBody(text: message.text, targets: targets).roomBubble()
        }
    }

    private var theirs: some View {
        HStack(alignment: .top, spacing: 10) {
            ZStack(alignment: .top) {
                if startsGroup {
                    RoomMessageFace(message: message, terminal: terminal)
                } else if hovered, let date {
                    RoomMessageTime(date: date).lineLimit(1).fixedSize().padding(.top, 3)
                }
            }
            .frame(width: 28)
            VStack(alignment: .leading, spacing: 3) {
                if startsGroup {
                    header
                }
                RoomMessageBody(text: message.text, targets: targets).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var header: some View {
        HStack(spacing: 6) {
            RoomMessageAuthor(message: message, handle: deps.rooms.member(message.authorId, in: roomId)?.handle)
            if let terminal {
                Button {
                    workspace.shownSection = 0
                    workspace.selectedTerminal = terminal.id
                    deps.activateSession(terminal.id, inRoom: roomId)
                } label: {
                    RoomTerminalChip(name: terminal.name)
                }
                .buttonStyle(PressScaleStyle()).help("Open this terminal")
            }
            if let date {
                RoomMessageTime(date: date)
            }
            Spacer(minLength: 0)
        }
    }

    private var actions: some View {
        HStack(spacing: 0) {
            IconButton(title: "Copy", symbol: "doc.on.doc", identifier: "room.message.copy", size: 24) {
                deps.copyText(message.text)
            }
            IconButton(title: madeTask ? "Added to tasks" : "Make this a task", symbol: madeTask ? "checkmark" : "checklist",
                       identifier: "room.message.task", size: 24, active: madeTask, action: makeTask)
                .disabled(madeTask || workspace.busy)
        }
        .padding(2)
        .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
    }

    private func makeTask() {
        Task { @MainActor in
            do {
                try await deps.rooms.makeTask(from: message, in: roomId)
                madeTask = true
            } catch { state.failure = error.localizedDescription }
        }
    }
}
