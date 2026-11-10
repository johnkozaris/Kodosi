import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct RoomMessageRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openURL) private var openURL
    let roomId: String
    let message: RoomMessage
    let previous: RoomMessage?
    let next: RoomMessage?
    let targets: [RoomMention]
    @State private var madeTask = 0
    @ScaledMetric(relativeTo: .caption) private var chipGlyph: CGFloat = 9

    private var authorHandle: String? {
        model.rooms.details[roomId]?.members.first { $0.userId == message.authorId }?.handle
    }

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
                Text(RoomDates.day(date))
                    .appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkFaint)
                    .padding(.horizontal, 10)
                    .frame(minHeight: 22)
                    .background(theme.colors.ink.opacity(0.06), in: Capsule())
                    .frame(maxWidth: .infinity)
                    .padding(.top, 16)
                    .padding(.bottom, 4)
                    .accessibilityAddTraits(.isHeader)
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

    private var messageText: some View {
        RoomMessageText(text: message.text, targets: targets)
            .appTextStyle(.body)
            .lineSpacing(2)
            .foregroundStyle(theme.colors.ink)
            .tint(theme.colors.accentStrong)
    }

    private var bubble: UnevenRoundedRectangle {
        UnevenRoundedRectangle(topLeadingRadius: 16, bottomLeadingRadius: 16, bottomTrailingRadius: 6, topTrailingRadius: 16, style: .continuous)
    }

    private var mine: some View {
        HStack(alignment: .bottom, spacing: 0) {
            Spacer(minLength: 56)
            VStack(alignment: .trailing, spacing: 3) {
                messageText
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(bubble.fill(theme.colors.accentSoft))
                    .contentShape(.contextMenuPreview, bubble)
                    .contextMenu { actions }
                if endsGroup, let date {
                    Text(date, format: .dateTime.hour().minute())
                        .appTextStyle(.caption2)
                        .foregroundStyle(theme.colors.inkFaint)
                        .padding(.trailing, 4)
                }
            }
        }
    }

    private var theirs: some View {
        let mentioned = RoomMentions.mentionsSelf(message.text, targets: targets)
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        return HStack(alignment: .top, spacing: 10) {
            avatar
                .frame(width: 28, height: startsGroup ? 28 : 1, alignment: .top)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                if startsGroup {
                    header
                }
                messageText.frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, mentioned ? 8 : 0)
        .padding(.vertical, mentioned ? 6 : 0)
        .background {
            if mentioned {
                shape.fill(theme.colors.accentSoft.opacity(0.6))
            }
        }
        .contentShape(.contextMenuPreview, shape)
        .contextMenu { actions }
    }

    @ViewBuilder
    private var avatar: some View {
        if !startsGroup {
            Color.clear
        } else if message.agent != nil {
            AgentMark(
                kind: AgentKind(agentName: message.agent, program: terminal?.program), size: 28,
                activity: terminal?.mark(rested: .awake) ?? .awake,
            )
        } else {
            PersonAvatar(name: message.authorName, key: message.authorId, size: 28, isSelf: message.authorId == model.rooms.userId)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(message.agent ?? message.authorName)
                    .appTextStyle(.subhead)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                if message.agent == nil, let authorHandle {
                    Text(verbatim: "@\(authorHandle)")
                        .appTextStyle(.caption)
                        .fontWeight(.regular)
                        .foregroundStyle(theme.colors.inkFaint)
                        .lineLimit(1)
                }
                if message.agent != nil {
                    Text("for \(message.authorName)")
                        .appTextStyle(.caption)
                        .fontWeight(.regular)
                        .foregroundStyle(theme.colors.inkFaint)
                        .lineLimit(1)
                }
                if let date {
                    Text(date, format: .dateTime.hour().minute())
                        .appTextStyle(.caption2)
                        .foregroundStyle(theme.colors.inkFaint)
                }
            }
            if let terminal {
                NavigationLink(value: TerminalRoute(sessionId: terminal.id)) {
                    HStack(spacing: 5) {
                        Image(systemName: "apple.terminal").font(.system(size: chipGlyph, weight: .bold))
                        Text(terminal.name).lineLimit(1)
                        if let sign = model.terminals.sign(of: terminal) {
                            StatusSign(form: sign, size: 9)
                        }
                    }
                    .appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkMuted)
                    .padding(.horizontal, 8)
                    .frame(minHeight: 22)
                    .background(theme.colors.ink.opacity(0.08), in: Capsule())
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
