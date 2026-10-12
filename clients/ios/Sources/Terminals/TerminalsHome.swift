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
        let needsYou = TerminalCatalog.needsYou(sessions)
        let groups = TerminalCatalog.groups(sessions)
        List {
            if !needsYou.isEmpty {
                Section {
                    ForEach(needsYou) { session in
                        needsYouCard(session)
                    }
                } header: {
                    SectionTitle(title: String(localized: "Needs you"), symbol: "hand.raised.fill", count: needsYou.count, tint: theme.colors.accent)
                }
            }
            ForEach(groups) { group in
                Section {
                    ForEach(group.sessions) { session in
                        row(session)
                    }
                } header: {
                    SectionTitle(title: group.title, symbol: group.symbol, count: group.sessions.count, tint: nil)
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background { GroundBackdrop() }
        .overlay { emptyContent(isEmpty: sessions.isEmpty) }
        .animation(theme.motion.soft, value: needsYou.map(\.id))
        .animation(theme.motion.soft, value: groups.map(\.id))
        .navigationTitle("Terminals")
        .searchable(text: $query, prompt: Text("Search terminals"))
        .refreshable { await store.refresh() }
        .navigationDestination(for: TerminalRoute.self) { route in
            TerminalScreen(sessionId: route.sessionId)
        }
        .terminalPrompts($prompt)
        .sensoryFeedback(.impact(weight: .medium), trigger: store.needsYou.map(\.id)) { old, new in
            !Set(new).subtracting(old).isEmpty
        }
        .sensoryFeedback(.success, trigger: finished) { old, new in
            !Set(new).subtracting(old).isEmpty
        }
        .accessibilityIdentifier("terminals.home")
    }

    private var finished: [String] {
        store.sessions.filter { $0.waitState == .done && store.attention.contains($0.id) }.map(\.id)
    }

    private func needsYouCard(_ session: RuntimeSession) -> some View {
        NeedsYouCard(session: session, sign: store.sign(of: session))
            .background {
                NavigationLink(value: TerminalRoute(sessionId: session.id)) { EmptyView() }
                    .opacity(0)
            }
            .listRowInsets(EdgeInsets(top: 6, leading: 0, bottom: 6, trailing: 0))
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
            .contextMenu { actions(for: session) }
            .transition(.asymmetric(insertion: .scale(scale: 0.96).combined(with: .opacity), removal: .opacity))
            .accessibilityIdentifier("terminals.needsYou.\(AccessibilityIdentifier.token(session.id))")
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
    let tint: Color?

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(tint ?? theme.colors.inkFaint)
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
