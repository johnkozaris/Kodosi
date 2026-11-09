import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openRoute) private var openRoute
    @State private var query = ""
    @State private var prompt: TerminalPrompt?

    private var store: TerminalStore {
        model.terminals
    }

    var body: some View {
        let sessions = TerminalCatalog.matching(store.sessions.filter { !$0.hidden }, query: query)
        let groups = TerminalCatalog.groups(sessions)
        List {
            ForEach(groups) { group in
                GroupHeader(group: group)
                    .cardRow(top: 16, bottom: 2)
                ForEach(group.sessions) { session in
                    row(session)
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(theme.colors.surface)
        .readableWidth()
        .overlay { emptyContent(isEmpty: sessions.isEmpty) }
        .animation(theme.motion.soft, value: groups.map(\.id))
        .navigationTitle("Terminals")
        .searchable(text: $query, prompt: Text("Search terminals"))
        .refreshable { await store.refresh() }
        .navigationDestination(for: TerminalRoute.self) { route in
            TerminalScreen(sessionId: route.sessionId)
        }
        .terminalPrompts($prompt)
        .sensoryFeedback(.impact(weight: .medium), trigger: store.listed ? store.needsYou.map(\.id) : nil) { old, new in
            arrived(old, new)
        }
        .sensoryFeedback(.success, trigger: store.listed ? finished : nil) { old, new in
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
        return TerminalListCard(session: session, sign: store.sign(of: session), viewers: model.viewers(of: session), showsOwner: false) {
            openRoute(TerminalRoute(sessionId: session.id))
        }
        .cardRow()
        .swipeActions(edge: .leading) {
            if session.isOwner {
                Button("Rename", systemImage: "pencil") { prompt = .rename(session) }
                    .tint(TileTint.graphite)
                    .accessibilityIdentifier("\(identifier).rename")
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if session.isOwner {
                Button("Close", systemImage: "xmark", role: .destructive) { prompt = .closing(session) }
                    .tint(theme.colors.danger)
                    .disabled(!session.canClose)
                    .accessibilityIdentifier("\(identifier).close")
            } else if session.canLeave {
                Button("Leave", systemImage: "rectangle.portrait.and.arrow.right", role: .destructive) { prompt = .leave(session) }
                    .tint(theme.colors.danger)
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
        if session.canClose {
            Button("Close terminal", systemImage: "xmark.circle", role: .destructive) { prompt = .closing(session) }
        }
    }

    @ViewBuilder
    private func emptyContent(isEmpty: Bool) -> some View {
        if isEmpty, !store.loaded || !store.listed {
            TerminalsLoading()
        } else if isEmpty, !query.isEmpty {
            NothingMatches()
        } else if isEmpty {
            TerminalsEmpty()
        }
    }
}

/// The name of a computer over its terminals, or the face and name of the friend who shares them.
private struct GroupHeader: View {
    @Environment(\.theme) private var theme
    let group: TerminalGroup

    var body: some View {
        HStack(spacing: 7) {
            ComputerCaption(title: group.title, ownerName: group.friend.map { _ in group.title }, ownerKey: group.friend,
                            away: group.away, size: 18, style: .subhead)
            Text(group.sessions.count, format: .number)
                .appTextStyle(.caption)
                .monospacedDigit()
                .foregroundStyle(theme.colors.inkFaint)
                .contentTransition(.numericText(value: Double(group.sessions.count)))
            Spacer(minLength: 0)
        }
        .foregroundStyle(theme.colors.inkMuted)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// What shows when a search finds nothing.
struct NothingMatches: View {
    @Environment(\.theme) private var theme

    var body: some View {
        Text("Nothing matches.")
            .appTextStyle(.footnote)
            .foregroundStyle(theme.colors.inkMuted)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .padding(.top, 40)
    }
}

extension View {
    /// A card in a list: on the list's ground, with room for its shadow and no separator.
    func cardRow(top: CGFloat = 5, bottom: CGFloat = 5) -> some View {
        listRowInsets(EdgeInsets(top: top, leading: 16, bottom: bottom, trailing: 16))
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
    }
}
