import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct RoomMessageRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL
    let roomId: String
    let message: RoomMessage
    let previous: RoomMessage?
    let next: RoomMessage?
    let targets: [RoomMention]
    @State private var madeTask = 0

    private var date: Date? {
        RoomDates.parse(message.createdAt)
    }

    private var isMine: Bool {
        model.rooms.isOwn(message)
    }

    private var terminal: RuntimeSession? {
        message.terminalId.flatMap { model.terminals.session($0) }
    }

    private var startsGroup: Bool {
        RoomDates.startsGroup(message, after: previous)
    }

    private var endsGroup: Bool {
        next.map { RoomDates.startsGroup($0, after: message) } ?? true
    }

    var body: some View {
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
            .padding(.top, startsGroup ? 12 : 3)
            .accessibilityAction(named: Text("Make a task"), makeTask)
        }
        .sensoryFeedback(.success, trigger: madeTask)
        .accessibilityIdentifier("rooms.message.\(AccessibilityIdentifier.token(message.id))")
    }

    private var mine: some View {
        HStack(alignment: .bottom, spacing: 0) {
            Spacer(minLength: 56)
            VStack(alignment: .trailing, spacing: 3) {
                RoomMessageBody(text: message.text, targets: targets)
                    .roomBubble()
                    .contentShape(.contextMenuPreview, RoomBubble())
                    .contextMenu { actions }
                if endsGroup, let date {
                    RoomMessageTime(date: date).padding(.trailing, 4)
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
                RoomMessageBody(text: message.text, targets: targets).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .roomMention(RoomMentions.mentionsSelf(message.text, targets: targets))
        .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .contextMenu { actions }
    }

    @ViewBuilder
    private var avatar: some View {
        if startsGroup {
            RoomMessageFace(message: message, terminal: terminal)
        } else {
            Color.clear
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                RoomMessageAuthor(message: message, handle: model.rooms.member(message.authorId, in: roomId)?.handle)
                if let date {
                    RoomMessageTime(date: date)
                }
            }
            if let terminal {
                NavigationLink(value: TerminalRoute(sessionId: terminal.id)) {
                    RoomTerminalChip(name: terminal.name, sign: model.terminals.sign(of: terminal))
                        .touchTarget(Capsule())
                }
                .buttonStyle(PressScaleStyle())
                .accessibilityLabel(Text("Open \(terminal.name)"))
                .accessibilityIdentifier("rooms.message.terminal.\(AccessibilityIdentifier.token(message.id))")
            }
        }
    }

    @ViewBuilder
    private var actions: some View {
        Button { UIPasteboard.general.string = message.text } label: {
            Label("Copy", systemImage: "doc.on.doc")
        }
        Button {
            model.roomScreen(roomId).selectedText = RoomTextSelection(id: message.id, text: message.text)
        } label: {
            Label("Select text", systemImage: "selection.pin.in.out")
        }
        Button(action: makeTask) {
            Label("Make a task", systemImage: "checklist")
        }
        if let terminal, let link = RoomMentions.terminalLink(terminal.id) {
            Button { openURL(link) } label: {
                Label("Open \(terminal.name)", systemImage: "apple.terminal")
            }
        }
    }

    private func makeTask() {
        let rooms = model.rooms
        Task {
            do {
                try await rooms.makeTask(from: message, in: roomId)
                madeTask += 1
            } catch {
                rooms.report(error)
            }
        }
    }
}
