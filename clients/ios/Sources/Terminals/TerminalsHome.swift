import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var query = ""
    @State private var prompt: TerminalPrompt?

    private var store: TerminalStore {
        model.terminals
    }

    var body: some View {
        let sessions = TerminalCatalog.matching(store.sessions, query: query)
        let groups = TerminalCatalog.groups(sessions)
        List {
            ForEach(groups) { group in
                Section {
                    ForEach(group.sessions) { session in
                        row(session)
                    }
                } header: {
                    SectionTitle(title: group.title, symbol: group.symbol, count: group.sessions.count)
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background { GroundBackdrop() }
        .overlay { emptyContent(isEmpty: sessions.isEmpty) }
        .animation(theme.motion.soft, value: groups.map(\.id))
        .navigationTitle("Terminals")
        .searchable(text: $query, prompt: Text("Search terminals"))
        .refreshable { await store.refresh() }
        .navigationDestination(for: TerminalRoute.self) { route in
            TerminalScreen(sessionId: route.sessionId)
        }
        .terminalPrompts($prompt)
        .sensoryFeedback(.impact(weight: .medium), trigger: store.loaded ? store.needsYou.map(\.id) : nil) { old, new in
            arrived(old, new)
        }
        .sensoryFeedback(.success, trigger: store.loaded ? finished : nil) { old, new in
            arrived(old, new)
        }
        .accessibilityIdentifier("terminals.home")
    }

    private func arrived(_ old: [String]?, _ new: [String]?) -> Bool {
        guard let old, let new else { return false }
        return !Set(new).subtracting(old).isEmpty
    }

    private var finished: [String] {
        store.sessions.filter { $0.waitState == .done && $0.attention }.map(\.id)
    }

    private func row(_ session: RuntimeSession) -> some View {
        let identifier = "terminals.row.\(AccessibilityIdentifier.token(session.id))"
        return NavigationLink(value: TerminalRoute(sessionId: session.id)) {
            TerminalRow(session: session, sign: store.sign(of: session), viewers: model.viewers(of: session))
        }
        .listRowBackground(theme.colors.raised)
        .swipeActions(edge: .leading) {
            if session.isOwner {
                Button("Rename", systemImage: "pencil") { prompt = .rename(session) }
                    .tint(theme.colors.accent)
                    .accessibilityIdentifier("\(identifier).rename")
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if session.isOwner {
                Button("Close", systemImage: "xmark", role: .destructive) { prompt = .close(session) }
                    .disabled(!session.canControl)
                    .accessibilityIdentifier("\(identifier).close")
            } else if session.canLeave {
                Button("Leave", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) { prompt = .leave(session) }
                    .accessibilityIdentifier("\(identifier).leave")
            }
        }
        .contextMenu { actions(for: session) }
        .accessibilityIdentifier(identifier)
    }

    @ViewBuilder
    private func actions(for session: RuntimeSession) -> some View {
        if session.isOwner {
            Button("Rename", systemImage: "pencil") { prompt = .rename(session) }
        }
        if session.canControl {
            Button("Interrupt", systemImage: "stop.circle") { store.interrupt(session) }
        }
        if session.canLeave {
            Button("Leave", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) { prompt = .leave(session) }
        }
        if session.canControl {
            Button("Close terminal", systemImage: "xmark.circle", role: .destructive) { prompt = .close(session) }
        }
    }

    @ViewBuilder
    private func emptyContent(isEmpty: Bool) -> some View {
        if !store.loaded {
            TerminalsLoading()
        } else if isEmpty, !query.isEmpty {
            ContentUnavailableView.search(text: query)
        } else if isEmpty {
            TerminalsEmpty()
        }
    }
}

private struct SectionTitle: View {
    @Environment(\.theme) private var theme
    let title: String
    let symbol: String
    let count: Int

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.colors.inkFaint)
            Text(title)
                .appTextStyle(.subhead)
                .foregroundStyle(theme.colors.inkMuted)
            Text(count, format: .number)
                .appTextStyle(.caption)
                .monospacedDigit()
                .foregroundStyle(theme.colors.inkFaint)
                .contentTransition(.numericText(value: Double(count)))
        }
        .textCase(nil)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}
