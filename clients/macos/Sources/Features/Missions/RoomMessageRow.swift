import AppKit
import SwiftUI

enum RoomDates {
    static func parse(_ value: String) -> Date? {
        (try? Date(value, strategy: Date.ISO8601FormatStyle(includingFractionalSeconds: true)))
            ?? (try? Date(value, strategy: .iso8601))
    }

    static func day(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return String(localized: "Today")
        }
        if calendar.isDateInYesterday(date) {
            return String(localized: "Yesterday")
        }
        return date.formatted(.dateTime.weekday(.wide).month().day())
    }
}

struct RoomMessageRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let roomId: String
    let message: RoomMessage
    let previous: RoomMessage?
    let targets: [MentionTarget]
    @Bindable var state: RoomViewState
    @State private var hovered = false
    @State private var madeTask = false

    private var date: Date? {
        RoomDates.parse(message.createdAt)
    }

    private var isMine: Bool {
        message.authorId == deps.userId && message.agent == nil
    }

    private var terminal: RuntimeSession? {
        message.terminalId.flatMap(deps.session)
    }

    private var startsGroup: Bool {
        guard let previous, previous.authorId == message.authorId, previous.agent == message.agent else { return true }
        guard let date, let before = RoomDates.parse(previous.createdAt) else { return false }
        return date.timeIntervalSince(before) > 300
    }

    private var startsDay: Bool {
        guard let date else { return false }
        guard let previous, let before = RoomDates.parse(previous.createdAt) else { return true }
        return !Calendar.current.isDate(date, inSameDayAs: before)
    }

    var body: some View {
        let mentioned = !isMine && RoomMentions.mentionsSelf(message.text, targets: targets)
        VStack(spacing: 0) {
            if startsDay, let date {
                Text(RoomDates.day(date)).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                    .padding(.horizontal, 10).frame(height: 20).background(theme.colors.ink.opacity(0.06), in: Capsule())
                    .frame(maxWidth: .infinity).padding(.top, 14).padding(.bottom, 4)
            }
            Group {
                if isMine {
                    mine
                } else {
                    theirs
                }
            }
            .padding(.horizontal, mentioned ? 8 : 0).padding(.vertical, mentioned ? 6 : 0)
            .background {
                if mentioned {
                    RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(theme.colors.accentSoft.opacity(0.6))
                }
            }
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

    private var messageText: some View {
        RoomMessageText(text: message.text, targets: targets)
            .appTextStyle(.body).lineSpacing(2.5).foregroundStyle(theme.colors.ink).textSelection(.enabled)
            .tint(theme.colors.accentStrong)
    }

    private var mine: some View {
        HStack(alignment: .bottom, spacing: 6) {
            Spacer(minLength: 44)
            if hovered, let date {
                Text(date, style: .time).appTextStyle(.caption2).foregroundStyle(theme.colors.inkFaint).transition(.opacity)
            }
            messageText
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background {
                    UnevenRoundedRectangle(
                        topLeadingRadius: 16, bottomLeadingRadius: 16, bottomTrailingRadius: 6, topTrailingRadius: 16, style: .continuous
                    )
                    .fill(theme.colors.accentSoft)
                }
        }
    }

    private var theirs: some View {
        HStack(alignment: .top, spacing: 10) {
            ZStack(alignment: .top) {
                if startsGroup {
                    if message.agent != nil {
                        AgentMark(kind: AgentKind(agentName: message.agent, program: terminal?.program), size: 28,
                                  activity: terminal?.mark(rested: .awake) ?? .awake)
                    } else {
                        PersonAvatar(name: message.authorName, key: message.authorId, size: 28)
                    }
                } else if hovered, let date {
                    Text(date, format: .dateTime.hour().minute()).appTextStyle(.caption2).foregroundStyle(theme.colors.inkFaint)
                        .lineLimit(1).fixedSize().padding(.top, 3)
                }
            }
            .frame(width: 28)
            VStack(alignment: .leading, spacing: 3) {
                if startsGroup {
                    header
                }
                messageText.frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var header: some View {
        HStack(spacing: 6) {
            Text(message.agent ?? message.authorName).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
            if message.agent != nil {
                Text("for \(message.authorName)").appTextStyle(.caption).fontWeight(.regular)
                    .foregroundStyle(theme.colors.inkFaint).lineLimit(1)
            }
            if let terminal {
                Button {
                    state.canvas = 0
                    state.selectedTerminal = terminal.id
                    deps.activateSession(terminal.id, inRoom: roomId)
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: "apple.terminal").font(.system(size: 8, weight: .bold))
                        Text(terminal.name).lineLimit(1)
                    }
                    .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                    .padding(.horizontal, 7).frame(height: 18).background(theme.colors.ink.opacity(0.08), in: Capsule())
                }
                .buttonStyle(PressScaleStyle()).help("Open this terminal")
            }
            if let date {
                Text(date, style: .time).appTextStyle(.caption2).foregroundStyle(theme.colors.inkFaint)
            }
            Spacer(minLength: 0)
        }
    }

    private var actions: some View {
        HStack(spacing: 0) {
            IconButton(title: "Copy", symbol: "doc.on.doc", identifier: "room.message.copy", size: 24) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(message.text, forType: .string)
            }
            IconButton(title: madeTask ? "Added to tasks" : "Make this a task", symbol: madeTask ? "checkmark" : "checklist",
                       identifier: "room.message.task", size: 24, active: madeTask, action: makeTask)
                .disabled(madeTask || state.busy)
        }
        .padding(2)
        .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
    }

    private func makeTask() {
        let title = message.text.split(separator: "\n").first.map(String.init) ?? message.text
        Task { @MainActor in
            do {
                try await deps.roomAction(roomId, [
                    "type": .string("createTask"), "title": .string(String(title.prefix(120))),
                    "description": .string(String(localized: "From \(message.agent ?? message.authorName): \(message.text)")),
                    "repositoryIds": .array([]),
                ])
                madeTask = true
            } catch { state.failure = error.localizedDescription }
        }
    }
}
