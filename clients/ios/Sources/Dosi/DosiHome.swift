import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openRoute) private var openRoute
    @Environment(\.openURL) private var openURL
    let showTab: (MainTab) -> Void

    var body: some View {
        DosiScreen()
            .environment(place)
            .navigationDestination(for: TerminalRoute.self) { TerminalScreen(sessionId: $0.sessionId) }
            .navigationDestination(for: ProjectRoute.self) { ProjectScreen(projectId: $0.projectId) }
    }

    private var place: DosiPlace {
        let model = model
        return DosiPlace(
            dosi: model.dosi,
            terminal: { referred($0, in: model.terminals.sessions, id: \.id, name: \.name) },
            project: { referred($0, in: model.projects.entries, id: \.id, name: \.name) },
            sessions: { model.terminals.sessions.filter { !$0.hidden } },
            projects: { model.projects.entries },
            open: open,
            answer: model.answerDosi,
            send: { model.sendToDosi($0) },
            canSend: { model.dosiConnected && model.dosiComputer != nil },
        )
    }

    private func open(_ target: DosiTarget) {
        switch target.kind {
        case .terminal:
            if let session = referred(target.value, in: model.terminals.sessions, id: \.id, name: \.name) {
                openRoute(TerminalRoute(sessionId: session.id))
            }
        case .project:
            if let project = referred(target.value, in: model.projects.entries, id: \.id, name: \.name) {
                openRoute(ProjectRoute(projectId: project.id))
            }
        case .terminals:
            showTab(.terminals)
        case .projects:
            showTab(.projects)
        case .url:
            if let url = URL(string: target.value), ["http", "https"].contains(url.scheme?.lowercased() ?? "") {
                openURL(url)
            }
        case .agent, .file, .query, .command, .tool:
            break
        }
    }
}

private struct DosiScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @FocusState private var composing: Bool
    @State private var shown = false
    @State private var width: CGFloat = 0

    var body: some View {
        DosiThread(listening: composing)
            .contentMargins(.horizontal, max(0, (width - 760) / 2), for: .scrollContent)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            .scrollDismissesKeyboard(.interactively)
            .safeAreaBar(edge: .bottom) { DosiComposer(focused: $composing) }
            .background(theme.colors.ground)
            .navigationTitle("Dosi")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if !model.dosi.items.isEmpty {
                    ToolbarItem(placement: .primaryAction) { DosiNewChat() }
                        .sharedBackgroundVisibility(.hidden)
                }
            }
            .onAppear {
                shown = true
                see()
            }
            .onDisappear { shown = false }
            .onChange(of: model.dosi.items.last?.id) { see() }
    }

    /// The person's own message does not count as seen, so that the answer to it still counts on
    /// the tab when it comes after the person left.
    private func see() {
        guard shown, let last = model.dosi.items.last else { return }
        if case .person = last.body {
            return
        }
        model.dosi.see()
    }
}

private struct DosiNewChat: View {
    @Environment(AppModel.self) private var model
    @State private var asking = false

    var body: some View {
        IconButton(title: "New chat", symbol: "square.and.pencil", identifier: "dosi.new", size: 32) { asking = true }
            .disabled(!model.dosiConnected)
            .confirmationDialog("Start a new chat?", isPresented: $asking, titleVisibility: .visible) {
                Button("Start new chat", role: .destructive) { model.renewDosi() }
                    .accessibilityIdentifier("dosi.new.start")
            } message: {
                Text("Dosi forgets this chat, here and on your other computers.")
            }
    }
}
