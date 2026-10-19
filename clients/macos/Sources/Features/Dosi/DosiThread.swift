import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiRow: Identifiable, Equatable {
    enum Kind: Equatable {
        case divider(String)
        case item(DosiItem)
        case tools([DosiItem])
    }

    let id: String
    let kind: Kind
    let startsTurn: Bool

    static func rows(_ items: [DosiItem], now: Date = .now, calendar: Calendar = .current) -> [DosiRow] {
        var rows: [DosiRow] = []
        var previous: DosiItem?
        var shown: String?
        var index = items.startIndex
        while index < items.endIndex {
            let item = items[index]
            if let label = divider(after: previous, before: item, now: now, calendar: calendar), label != shown {
                rows.append(DosiRow(id: "divider.\(item.id)", kind: .divider(label), startsTurn: false))
                shown = label
            }
            let starts = startsTurn(item, after: previous)
            var run = [item]
            if case .tool = item.body {
                while index + 1 < items.endIndex, case .tool = items[index + 1].body, items[index + 1].turn == item.turn {
                    index += 1
                    run.append(items[index])
                }
            }
            rows.append(DosiRow(id: item.id, kind: run.count > 1 ? .tools(run) : .item(item), startsTurn: starts))
            previous = run.last
            index += 1
        }
        return rows
    }

    static func label(_ date: Date, now: Date, calendar: Calendar) -> String {
        if calendar.isDate(date, inSameDayAs: now) {
            let hour = calendar.component(.hour, from: date)
            if hour < 12 {
                return String(localized: "this morning")
            }
            return hour < 17 ? String(localized: "this afternoon") : String(localized: "this evening")
        }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(date, inSameDayAs: yesterday) {
            return String(localized: "yesterday")
        }
        if let days = calendar.dateComponents([.day], from: date, to: now).day, days < 7 {
            return date.formatted(.dateTime.weekday(.wide)).lowercased()
        }
        return date.formatted(.dateTime.day().month(.abbreviated))
    }

    private static func startsTurn(_ item: DosiItem, after previous: DosiItem?) -> Bool {
        guard let previous else { return true }
        if case .person = previous.body {
            return true
        }
        return previous.turn != item.turn
    }

    private static func divider(after previous: DosiItem?, before item: DosiItem, now: Date, calendar: Calendar) -> String? {
        if let previous, calendar.isDate(item.date, inSameDayAs: previous.date), item.date.timeIntervalSince(previous.date) < 3600 {
            return nil
        }
        return label(item.date, now: now, calendar: calendar)
    }
}

struct DosiThread: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let listening: Bool
    @State private var following = true
    @State private var viewport: CGFloat = 0
    @State private var phase = ScrollPhase.idle
    @State private var position = ScrollPosition(edge: .bottom)
    @State private var thinking = false

    private static let row = AnyTransition.asymmetric(
        insertion: .opacity.combined(with: .offset(y: 8)).combined(with: .scale(scale: 0.97, anchor: .bottom)),
        removal: .opacity.combined(with: .offset(y: -12))
    )

    var body: some View {
        let items = deps.dosi.items
        let rows = DosiRow.rows(items)
        let live = deps.dosi.busy ? items.last?.turn : nil
        let starts = items.last.map { last in
            if case .person = last.body {
                return true
            }
            return false
        } ?? true
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    VStack(alignment: .leading, spacing: 0) {
                        if deps.dosi.earlier, let first = items.first {
                            Button {
                                Task {
                                    try? await deps.dosi.loadEarlier()
                                    proxy.scrollTo(first.id, anchor: .top)
                                }
                            } label: { Label("Earlier", systemImage: "arrow.up") }
                                .buttonStyle(.kodosi(.ghost, size: .small)).frame(maxWidth: .infinity)
                                .disabled(deps.dosi.loadingEarlier).padding(.bottom, 4)
                                .accessibilityIdentifier("dosi.earlier")
                        }
                        ForEach(rows) { row in
                            DosiRowView(row: row, live: live).id(row.id).transition(Self.row)
                        }
                        if thinking, deps.dosi.pending {
                            DosiTurnRow(turn: "", starts: starts, live: "") { DosiThinking() }
                                .transition(.asymmetric(insertion: Self.row, removal: .opacity))
                        }
                        if let remote = deps.dosi.snapshot?.remote, deps.dosi.state == .working {
                            DosiAway(computer: remote.computer).padding(.top, 12).transition(AnyTransition.rise)
                        }
                    }
                    .padding(.horizontal, 14).padding(.top, 10).padding(.bottom, 8)
                    .frame(minHeight: max(viewport - 1, 0), alignment: .bottom)
                    .animation(theme.motion.spring, value: rows.map(\.id))
                    .animation(theme.motion.spring, value: deps.dosi.pending)
                    Color.clear.frame(height: 1).id("latest")
                }
            }
            .scrollPosition($position)
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            .scrollIndicators(.automatic)
            .onScrollGeometryChange(for: ScrollGeometry.self) { $0 } action: { old, new in
                viewport = new.containerSize.height
                let end = max(0, new.contentSize.height - new.containerSize.height)
                if new.contentSize != old.contentSize || new.containerSize != old.containerSize {
                    if following, abs(new.contentOffset.y - end) > 0.5 {
                        withTransaction(\.disablesAnimations, true) { position.scrollTo(y: end) }
                    }
                } else if new.contentOffset.y >= end - 40 {
                    following = true
                } else if old.contentOffset.y - new.contentOffset.y > ([.tracking, .interacting, .decelerating].contains(phase) ? 0.5 : 8) {
                    following = false
                }
            }
            .onScrollPhaseChange { _, new in phase = new }
            .task(id: deps.dosi.pending) {
                guard deps.dosi.pending else {
                    thinking = false
                    return
                }
                if !starts {
                    try? await Task.sleep(for: .milliseconds(600))
                }
                if !Task.isCancelled {
                    withAnimation(theme.motion.spring) { thinking = true }
                }
            }
            .onChange(of: deps.dosi.snapshot?.conversation?.id) { _, _ in
                following = true
            }
            .overlay {
                if items.isEmpty {
                    DosiWelcome(listening: listening)
                }
            }
            .overlay(alignment: .bottom) {
                ZStack {
                    if !following {
                        Button {
                            following = true
                            withAnimation(theme.motion.soft) { proxy.scrollTo("latest", anchor: .bottom) }
                        } label: { Label("Latest", systemImage: "arrow.down") }
                            .buttonStyle(.kodosi(.secondary, size: .small)).padding(8)
                            .transition(.move(edge: .bottom).combined(with: .opacity))
                    }
                }
                .animation(theme.motion.spring, value: following)
            }
        }
    }
}

struct DosiRowView: View {
    let row: DosiRow
    let live: String?

    var body: some View {
        switch row.kind {
        case let .divider(label):
            DosiDivider(label: label).padding(.top, 14).padding(.bottom, 2)
        case let .item(item):
            if case let .person(text) = item.body {
                DosiPersonBubble(text: text).padding(.top, 12)
            } else {
                DosiTurnRow(turn: item.turn, starts: row.startsTurn, live: live) { DosiItemContent(item: item) }
            }
        case let .tools(items):
            DosiTurnRow(turn: items[0].turn, starts: row.startsTurn, live: live) { DosiToolGroup(items: items) }
        }
    }
}

struct DosiTurnRow<Content: View>: View {
    @Environment(AppDependencies.self) private var deps
    let turn: String
    let starts: Bool
    let live: String?
    @ViewBuilder let content: Content

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            ZStack {
                if starts {
                    DosiFace(mood: live == turn ? deps.dosi.mood() : .resting, size: 20, still: true)
                }
            }
            .frame(width: 20)
            content.frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.top, starts ? 12 : 6)
    }
}

struct DosiItemContent: View {
    @Environment(\.theme) private var theme
    let item: DosiItem

    var body: some View {
        switch item.body {
        case let .text(text, live):
            DosiProse(text: text, live: live)
        case let .reasoning(text, live, seconds):
            DosiThought(text: text, live: live, seconds: seconds)
        case let .tool(tool):
            DosiReceipt(tool: tool)
        case let .question(question):
            DosiQuestionView(item: item, question: question)
        case let .note(text):
            Text(text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
        case let .notice(notice):
            DosiNoticeView(notice: notice)
        case .person:
            EmptyView()
        }
    }
}

struct DosiDivider: View {
    @Environment(\.theme) private var theme
    let label: String

    var body: some View {
        HStack(spacing: 8) {
            Rectangle().fill(theme.colors.hairline.mix(with: theme.colors.ground, by: 0.55)).frame(height: 1)
            Text(label).font(.system(size: 10, design: .monospaced)).tracking(0.4).foregroundStyle(theme.colors.inkFaint)
                .fixedSize()
            Rectangle().fill(theme.colors.hairline.mix(with: theme.colors.ground, by: 0.55)).frame(height: 1)
        }
        .frame(height: 16)
        .accessibilityElement(children: .combine)
    }
}
