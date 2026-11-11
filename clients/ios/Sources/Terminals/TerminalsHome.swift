import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openRoute) private var openRoute
    @State private var query = ""
    @State private var prompt: TerminalPrompt?
    @State private var newTerminal: NewTerminalRequest?

    private var store: TerminalStore {
        model.terminals
    }

    var body: some View {
        let sessions = TerminalCatalog.matching(store.sessions.filter { !$0.hidden }, query: query)
        let computers = SessionComputerGroup.groups(sessions)
        let online = model.account.computers.map(\.deviceId)
        List {
            ForEach(computers) { computer in
                ComputerHeader(group: computer, newTerminal: starting(TerminalCatalog.newTerminal(on: computer, online: online)))
                    .cardRow(top: computer.id == computers.first?.id ? 4 : 18, bottom: 0)
                ForEach(computer.folders) { folder in
                    if folder.directory != nil {
                        FolderHeader(folder: folder, newTerminal: starting(TerminalCatalog.newTerminal(in: folder, online: online)))
                            .cardRow(top: folder.id == computer.folders.first?.id ? 0 : 6, bottom: 0, indent: Self.folderIndent)
                    }
                    ForEach(folder.sessions) { session in
                        row(session, indent: folder.directory == nil ? 0 : Self.folderIndent)
                    }
                }
            }
        }
        .listStyle(.plain)
        .environment(\.defaultMinListRowHeight, 0)
        .scrollContentBackground(.hidden)
        .background(theme.colors.surface)
        .readableWidth()
        .overlay { emptyContent(isEmpty: sessions.isEmpty) }
        .animation(theme.motion.soft, value: computers.flatMap { $0.folders.map(\.id) })
        .navigationTitle("Terminals")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                IconButton(title: "New terminal", symbol: "plus", identifier: "terminals.new", size: 32) {
                    newTerminal = NewTerminalRequest()
                }
            }
            .sharedBackgroundVisibility(.hidden)
        }
        .searchable(text: $query, prompt: Text("Search terminals"))
        .refreshable { await store.refresh() }
        .navigationDestination(for: TerminalRoute.self) { route in
            TerminalScreen(sessionId: route.sessionId)
        }
        .terminalPrompts($prompt)
        .newTerminalSheet($newTerminal)
        .sensoryFeedback(.impact(weight: .medium), trigger: store.listed ? store.needsYou.map(\.id) : nil) { old, new in
            arrived(old, new)
        }
        .sensoryFeedback(.success, trigger: store.listed ? finished : nil) { old, new in
            arrived(old, new)
        }
        .accessibilityIdentifier("terminals.home")
    }

    private static let folderIndent: CGFloat = 8

    private func starting(_ request: NewTerminalRequest?) -> (() -> Void)? {
        request.map { request in { newTerminal = request } }
    }

    private func arrived(_ old: [String]?, _ new: [String]?) -> Bool {
        guard let old, let new else { return false }
        return !Set(new).subtracting(old).isEmpty
    }

    private var finished: [String] {
        store.sessions.filter { $0.waiting == .done && $0.attention }.map(\.id)
    }

    private func row(_ session: RuntimeSession, indent: CGFloat) -> some View {
        let identifier = "terminals.row.\(AccessibilityIdentifier.token(session.id))"
        return TerminalListCard(
            session: session, sign: store.sign(of: session), viewers: model.viewers(of: session), showsOwner: false, showsFolder: false,
        ) {
            openRoute(TerminalRoute(sessionId: session.id))
        }
        .cardRow(indent: indent)
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

private struct ComputerHeader: View {
    @Environment(\.theme) private var theme
    let group: SessionComputerGroup
    let newTerminal: (() -> Void)?

    var body: some View {
        let count = group.sessions.count
        HStack(spacing: 7) {
            HStack(spacing: 7) {
                ComputerCaption(title: group.title, ownerName: group.owner?.name, ownerKey: group.owner?.userId, away: group.away, size: 18,
                                style: .subhead)
                Text(count, format: .number)
                    .appTextStyle(.caption)
                    .monospacedDigit()
                    .foregroundStyle(theme.colors.inkFaint)
                    .contentTransition(.numericText(value: Double(count)))
                    .fixedSize()
            }
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isHeader)
            .accessibilityIdentifier("terminals.computer.\(AccessibilityIdentifier.token(group.id))")
            Spacer(minLength: 0)
            if let newTerminal {
                IconButton(
                    title: "New terminal", symbol: "plus", identifier: "terminals.group.new.\(AccessibilityIdentifier.token(group.id))", size: 28,
                    action: newTerminal,
                )
                .accessibilityValue(Text(verbatim: group.title))
            }
        }
        .foregroundStyle(theme.colors.inkMuted)
        .frame(minHeight: 44)
    }
}

private struct FolderHeader: View {
    @Environment(\.theme) private var theme
    let folder: SessionFolderGroup
    let newTerminal: (() -> Void)?

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "folder.fill")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(theme.colors.inkFaint)
            Text(folder.name)
                .appTextStyle(.footnote)
                .fontWeight(.medium)
                .foregroundStyle(theme.colors.inkMuted)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .frame(minHeight: 30)
        .contentShape(Rectangle())
        .contextMenu {
            if let newTerminal {
                Button("New terminal here", systemImage: "plus", action: newTerminal)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
        .accessibilityActions {
            if let newTerminal {
                Button("New terminal here", action: newTerminal)
            }
        }
        .accessibilityIdentifier("terminals.folder.\(AccessibilityIdentifier.token(folder.id))")
    }
}

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
    func cardRow(top: CGFloat = 5, bottom: CGFloat = 5, indent: CGFloat = 0) -> some View {
        listRowInsets(EdgeInsets(top: top, leading: 16 + indent, bottom: bottom, trailing: 16))
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
    }
}
