import KodosiBridge
import SwiftUI

struct ChatLine: Identifiable, Equatable {
    enum Kind: Equatable {
        case day(Date)
        case row(ChatRow, lead: Bool)
        case now(lead: Bool)
    }

    let id: String
    let kind: Kind

    static func lines(_ rows: [ChatRow], now: Bool) -> [ChatLine] {
        var lines: [ChatLine] = []
        var day: Date?
        var agentBefore = false
        for row in rows {
            if let date = row.date {
                if let day, !Calendar.current.isDate(day, inSameDayAs: date) {
                    lines.append(ChatLine(id: "day.\(row.id)", kind: .day(date)))
                    agentBefore = false
                }
                day = date
            }
            let agent = row.isAgents
            lines.append(ChatLine(id: row.id, kind: .row(row, lead: agent && !agentBefore)))
            agentBefore = agent
        }
        if now {
            lines.append(ChatLine(id: "now", kind: .now(lead: !agentBefore)))
        }
        return lines
    }

    var isPerson: Bool {
        if case let .row(row, _) = kind, case .person = row.kind {
            return true
        }
        return false
    }

    var lead: Bool {
        switch kind {
        case let .row(_, lead), let .now(lead): lead
        case .day: false
        }
    }
}

extension ChatRow {
    /// Rows emitted before the agent connection reports timestamps have `at == 0`.
    var date: Date? {
        at > 0 ? Date(timeIntervalSince1970: TimeInterval(at) / 1000) : nil
    }

    var isAgents: Bool {
        switch kind {
        case .person: false
        case let .note(note): note != .compacted
        case .answer, .plan, .work, .end: true
        }
    }
}

/// Gives the latest user message and everything after it a minimum height equal to the viewport,
/// so sending a message scrolls it to the top and lets the response grow below it.
struct ChatThread: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let agent: AgentKind
    let me: String?
    /// Incremented on each local send to trigger scrolling to the bottom.
    let sent: Int
    @State private var following = true
    /// Tracks whether the last actual row is visible, separate from `following` because the
    /// viewport-tall trailing section can leave empty space below it.
    @State private var endShown = true
    @State private var viewport: CGFloat = 0
    @State private var position = ScrollPosition(edge: .bottom)

    private static let row = AnyTransition.asymmetric(
        insertion: .opacity.combined(with: .offset(y: 8)).combined(with: .scale(scale: 0.97, anchor: .bottom)),
        removal: .opacity,
    )

    var body: some View {
        let rows = ChatRow.rows(chat.items, live: chat.working)
        let lines = ChatLine.lines(rows, now: showsNow(rows))
        let start = lines.lastIndex(where: \.isPerson) ?? lines.endIndex
        let newest = start < lines.endIndex ? lines[start].id : nil
        let working = chat.state == .working ? lines.last(where: \.lead)?.id : nil
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(lines[..<start]) { line in
                            lineView(line, working: line.id == working).id(line.id).transition(Self.row)
                        }
                    }
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(lines[start...]) { line in
                            lineView(line, working: line.id == working).id(line.id).transition(Self.row)
                        }
                        Color.clear.frame(height: 1).onScrollVisibilityChange(threshold: 0.5) { endShown = $0 }
                    }
                    .frame(minHeight: start < lines.endIndex ? max(viewport - 1, 0) : nil, alignment: .top)
                    Color.clear.frame(height: 1).id("latest")
                }
                .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 12)
                .chatColumn()
                .animation(theme.motion.spring, value: lines.map(\.id))
            }
            .followsEnd($following, position: $position, viewport: $viewport)
            .scrollIndicators(.automatic)
            .scrollDismissesKeyboard(.interactively)
            .overlay {
                let empty = chat.chat != nil && rows.isEmpty && !chat.working
                ZStack {
                    if empty {
                        AgentMark(kind: agent, size: 40).accessibilityHidden(true).transition(.opacity)
                    }
                }
                .animation(theme.motion.fade, value: empty)
            }
            .overlay(alignment: .bottom) {
                ZStack {
                    if !following, !endShown {
                        LatestKey { toEnd(proxy) }
                    }
                }
                .animation(theme.motion.spring, value: !following && !endShown)
            }
            .onChange(of: sent) { _, _ in toEnd(proxy) }
            // Scroll queued or cross-device user messages to the top if the user is at the end.
            .onChange(of: newest) { _, _ in
                if following || endShown {
                    toEnd(proxy)
                }
            }
        }
    }

    private func showsNow(_ rows: [ChatRow]) -> Bool {
        guard chat.working, chat.asks.isEmpty else { return false }
        switch rows.last?.kind {
        case let .answer(_, live), let .plan(_, live): return !live
        default: return true
        }
    }

    @ContentBuilder
    private func lineView(_ line: ChatLine, working: Bool) -> some View {
        switch line.kind {
        case let .day(date):
            ProjectDayDivider(date: date)
        case let .row(row, lead):
            if row.isAgents {
                AgentTurnPart(agent: agent, lead: lead, time: row.date, working: working) {
                    ChatRowView(row: row, me: me, hold: hold)
                }
            } else {
                ChatRowView(row: row, me: me, hold: hold)
            }
        case let .now(lead):
            AgentTurnPart(agent: agent, lead: lead, time: .now, working: true) {
                ChatNowLine(chat: chat).padding(.top, lead ? 2 : 8)
            }
        }
    }

    private func toEnd(_ proxy: ScrollViewProxy) {
        following = true
        withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
    }

    /// Stops auto-scrolling when the user expands a step so the viewport stays anchored.
    private func hold() {
        following = false
    }
}

private struct AgentTurnPart<Content: View>: View {
    #if os(iOS)
        private static var face: CGFloat {
            28
        }
    #else
        private static var face: CGFloat {
            26
        }
    #endif
    private static var gap: CGFloat {
        10
    }

    @Environment(\.theme) private var theme
    let agent: AgentKind
    let lead: Bool
    let time: Date?
    let working: Bool
    @ContentBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if lead {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(verbatim: agent.label).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    if let time {
                        ProjectMessageTime(date: time)
                    }
                }
                .padding(.top, 4)
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(.isHeader)
            }
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.leading, Self.face + Self.gap)
        .overlay(alignment: .topLeading) {
            if lead {
                AgentMark(kind: agent, size: Self.face, activity: working ? .working : .awake)
                    .padding(.top, 2)
                    .accessibilityHidden(true)
            }
        }
        .padding(.top, lead ? 16 : 0)
    }
}

struct ChatRowView: View {
    @Environment(\.theme) private var theme
    let row: ChatRow
    let me: String?
    let hold: () -> Void

    var body: some View {
        switch row.kind {
        case let .person(text, by):
            if let by, let me, by.id != me {
                ChatOtherPerson(text: text, person: by, time: row.date).copiesMessage(text).padding(.top, 16)
            } else {
                PersonBubble(text: text, copies: true).padding(.top, 18)
            }
        case let .answer(text, live):
            AgentProse(text: text, live: live).frame(maxWidth: .infinity, alignment: .leading)
                .copiesMessage(live ? nil : text, at: .bottom).padding(.top, 6)
        case let .plan(text, live):
            VStack(alignment: .leading, spacing: 6) {
                Label { Text(verbatim: ChatWords.mode(.plan)) } icon: { Image(systemName: ChatWords.symbol(.plan)) }
                    .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                AgentProse(text: text, live: live).padding(12).frame(maxWidth: .infinity, alignment: .leading)
                    .raised(Radius.lg, fill: theme.colors.raised)
            }
            .copiesMessage(live ? nil : text, at: .bottom)
            .padding(.top, 8)
        case let .work(items):
            ChatWork(items: items, hold: hold).padding(.top, 4)
        case let .end(end, ms):
            ChatTurnEnd(end: end, ms: ms).padding(.top, 8)
        case let .note(note):
            ChatNoteView(note: note).padding(.top, 10)
        }
    }
}

private struct ChatOtherPerson: View {
    @Environment(\.theme) private var theme
    let text: String
    let person: ChatPerson
    let time: Date?

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            PersonAvatar(name: person.name, key: person.id, size: 28).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(verbatim: person.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    if let time {
                        ProjectMessageTime(date: time)
                    }
                }
                Text(ProjectMessageText.formatted(text))
                    .appTextStyle(.body).lineSpacing(2).foregroundStyle(theme.colors.ink).messageTextSelection()
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

struct ChatNowLine: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat

    var body: some View {
        let words = ChatWords.now(chat.now, in: chat)
        ZStack(alignment: .leading) {
            Text(verbatim: words).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle)
                .id(words)
                .transition(AnyTransition.roll)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .controlHeight(20)
        .clipped()
        .animation(theme.motion.snappy, value: words)
        .accessibilityIdentifier("chat.live")
    }
}

struct ChatTurnEnd: View {
    @Environment(\.theme) private var theme
    let end: TurnEnd
    let ms: UInt64

    var body: some View {
        HStack(spacing: 8) {
            switch end {
            case .stopped: tag(Text("Stopped", bundle: .module))
            case .failed: tag(Text("Failed", bundle: .module))
            case .refused: tag(Text("Refused", bundle: .module))
            case .done: EmptyView()
            }
            if ms >= ChatRow.longTurn {
                Text("worked for \(ChatWords.duration(ms))", bundle: .module)
                    .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func tag(_ text: Text) -> some View {
        text.appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            .padding(.horizontal, 9).controlHeight(20)
            .raisedCapsule(fill: theme.colors.raised)
    }
}

struct ChatNoteView: View {
    @Environment(\.theme) private var theme
    let note: ChatNote

    var body: some View {
        switch note {
        case .compacted:
            Text("compacted", bundle: .module)
                .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                .padding(.horizontal, 10).controlHeight(22)
                .raisedCapsule(fill: theme.colors.raised)
                .frame(maxWidth: .infinity)
        case .signedOut:
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                StatusSign(form: .key, size: 12)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Needs you to sign in", bundle: .module).appTextStyle(.footnote).fontWeight(.semibold)
                        .foregroundStyle(theme.colors.ink)
                    Text("Sign in with its own steps in the terminal.", bundle: .module).appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                }
            }
        case let .recap(text):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "text.append").font(mac: 11, ios: .footnote, weight: .semibold)
                    .foregroundStyle(theme.colors.inkFaint).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Recap", bundle: .module).appTextStyle(.footnote).fontWeight(.semibold)
                        .foregroundStyle(theme.colors.inkMuted)
                    Text(verbatim: text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .textSelection(.enabled)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
        case let .failed(error, text):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                StatusSign(form: .failed, size: 12)
                VStack(alignment: .leading, spacing: 4) {
                    Text(verbatim: error).appTextStyle(.footnote).fontWeight(.semibold).foregroundStyle(theme.colors.ink)
                        .textSelection(.enabled)
                    if !text.isEmpty {
                        Text(text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                    }
                }
            }
        }
    }
}

extension View {
    func chatColumn() -> some View {
        frame(maxWidth: 760).frame(maxWidth: .infinity)
    }
}
