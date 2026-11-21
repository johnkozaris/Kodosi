import KodosiBridge
import SwiftUI

/// The newest message of a person starts a part as high as the view, so that after Send the
/// message moves to the top and the answer grows below it.
struct ChatThread: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    let agent: AgentKind
    let me: String?
    /// Counts the messages that a person sent from this view.
    let sent: Int
    @State private var following = true
    /// The last row is in view. Below it the part of the newest message can be empty.
    @State private var endShown = true
    @State private var viewport: CGFloat = 0
    @State private var position = ScrollPosition(edge: .bottom)

    private static let row = AnyTransition.asymmetric(
        insertion: .opacity.combined(with: .offset(y: 8)).combined(with: .scale(scale: 0.97, anchor: .bottom)),
        removal: .opacity,
    )

    var body: some View {
        let rows = ChatRow.rows(chat.items, live: chat.working)
        let start = rows.lastIndex {
            if case .person = $0.kind {
                true
            } else {
                false
            }
        } ?? rows.endIndex
        let newest = start < rows.endIndex ? rows[start].id : nil
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(rows[..<start]) { row in
                            ChatRowView(row: row, me: me, hold: hold).id(row.id).transition(Self.row)
                        }
                    }
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(rows[start...]) { row in
                            ChatRowView(row: row, me: me, hold: hold).id(row.id).transition(Self.row)
                        }
                        Color.clear.frame(height: 1).onScrollVisibilityChange(threshold: 0.5) { endShown = $0 }
                    }
                    .frame(minHeight: start < rows.endIndex ? max(viewport - 1, 0) : nil, alignment: .top)
                    Color.clear.frame(height: 1).id("latest")
                }
                .padding(.horizontal, 14).padding(.top, 6).padding(.bottom, 10)
                .chatColumn()
                .animation(theme.motion.spring, value: rows.map(\.id))
            }
            .followsEnd($following, position: $position, viewport: $viewport)
            .scrollIndicators(.automatic)
            .scrollDismissesKeyboard(.interactively)
            .overlay {
                let empty = chat.chat != nil && rows.isEmpty
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
            // A message that starts later, from the queue or from another device, also goes to the
            // top for a person who is at the end.
            .onChange(of: newest) { _, _ in
                if following || endShown {
                    toEnd(proxy)
                }
            }
        }
    }

    private func toEnd(_ proxy: ScrollViewProxy) {
        following = true
        withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
    }

    /// The steps that a person opens grow below the line that they pressed, and the view stays
    /// where it is.
    private func hold() {
        following = false
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
            VStack(alignment: .trailing, spacing: 4) {
                if let by, let me, by.id != me {
                    HStack(spacing: 5) {
                        PersonAvatar(name: by.name, key: by.id, size: 16)
                        Text(verbatim: by.name).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                    }
                    .accessibilityElement(children: .combine)
                }
                PersonBubble(text: text)
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
            .padding(.top, 16)
        case let .answer(text, live):
            AgentProse(text: text, live: live).padding(.top, 10)
        case let .plan(text, live):
            VStack(alignment: .leading, spacing: 6) {
                Label { Text(verbatim: ChatWords.mode(.plan)) } icon: { StepGlyph(symbol: ChatWords.symbol(.plan)) }
                    .font(mac: 11, ios: .caption, design: .monospaced).foregroundStyle(theme.colors.inkMuted)
                AgentProse(text: text, live: live).padding(10).frame(maxWidth: .infinity, alignment: .leading).well(Radius.md)
            }
            .padding(.top, 10)
        case let .work(items):
            ChatWork(items: items, hold: hold).padding(.top, 10)
        case let .end(end, ms):
            ChatTurnEnd(end: end, ms: ms).padding(.top, 8)
        case let .note(note):
            ChatNoteView(note: note).padding(.top, 12)
        }
    }
}

struct ChatTurnEnd: View {
    @Environment(\.theme) private var theme
    let end: TurnEnd
    let ms: UInt64

    var body: some View {
        HStack(spacing: 6) {
            switch end {
            case .stopped: tag(Text("Stopped", bundle: .module))
            case .failed: tag(Text("Failed", bundle: .module))
            case .refused: tag(Text("Refused", bundle: .module))
            case .done: EmptyView()
            }
            if ms >= ChatRow.longTurn {
                Text("worked for \(ChatWords.duration(ms))", bundle: .module)
                    .font(mac: 10.5, ios: .caption2, design: .monospaced).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private func tag(_ text: Text) -> some View {
        text.font(mac: 10.5, ios: .caption2, design: .monospaced).foregroundStyle(theme.colors.inkFaint)
            .padding(.horizontal, 8).controlHeight(18).background(theme.colors.surface, in: Capsule())
    }
}

struct ChatNoteView: View {
    @Environment(\.theme) private var theme
    let note: ChatNote

    var body: some View {
        switch note {
        case .compacted:
            LabeledDivider(label: String(localized: "compacted", bundle: .module))
        case let .failed(error, text):
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    StatusSign(form: .failed, size: 12)
                    Text(verbatim: error).appTextStyle(.monoCaption).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                }
                if !text.isEmpty {
                    Text(text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .raised(Radius.lg, fill: theme.colors.raised, elevation: .resting)
        }
    }
}

extension View {
    /// A chat reads as one centered column, also in a wide tile.
    func chatColumn() -> some View {
        frame(maxWidth: 760).frame(maxWidth: .infinity)
    }
}
