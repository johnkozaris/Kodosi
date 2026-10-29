import KodosiBridge
import KodosiDesign
import SwiftUI

/// The canvas key in the navigation bar: there when the canvas shows something, with a
/// breathing dot for news.
struct CanvasKey: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let canvas: CanvasStore
    let open: () -> Void

    var body: some View {
        let news = canvas.hasNews(session)
        Button(action: open) {
            Image(systemName: CanvasSymbol.canvas)
                .overlay(alignment: .topTrailing) {
                    if news {
                        BreathingDot(color: theme.colors.glowOrange, size: 7).offset(x: 5, y: -4).transition(AnyTransition.pop)
                    }
                }
                .symbolEffect(.bounce, value: news)
        }
        .accessibilityLabel(Text("Canvas"))
        .accessibilityValue(news ? Text("Something new") : Text(verbatim: ""))
        .accessibilityIdentifier("terminal.canvas")
    }
}

/// The canvas of a terminal full screen: its parts in the bar, the files as tabs or cards, and
/// the part that it shows.
struct CanvasScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    @State private var frame: CanvasFrame?
    @State private var typing = false

    private var store: CanvasStore {
        model.terminals.canvas
    }

    var body: some View {
        let session = model.terminals.session(sessionId)
        let current = session.flatMap(store.current)
        let shelf = current?.section == .files && store.showsShelf(sessionId)
        NavigationStack {
            VStack(spacing: 0) {
                if let session, current?.section == .files {
                    CanvasFilesBar(session: session, store: store, chosen: current?.key) { key in
                        store.pick(key, in: session.id)
                    } remove: { item in
                        remove([item.key], from: session)
                    } actions: { item in
                        actions(item, in: session)
                    }
                    Rectangle().fill(theme.colors.hairline.opacity(0.6)).frame(height: 0.5)
                }
                ZStack {
                    if let session, shelf {
                        CanvasShelf(session: session, store: store) { key in
                            store.pick(key, in: session.id)
                        } remove: { item in
                            remove([item.key], from: session)
                        } actions: { item in
                            actions(item, in: session)
                        }
                        .transition(.opacity)
                    } else if let session, let current, let frame {
                        CanvasItemView(session: session, shown: current, store: store, frame: frame, typing: typing,
                                       ask: ask(current, in: session))
                        {
                            remove([CanvasShown.browserKey], from: session)
                        }
                        .id(current.key)
                        .transition(.opacity)
                    } else if store.opensBrowser(sessionId) {
                        Spinner(size: 22)
                    } else if session?.canvas == nil {
                        ContentUnavailableView("The canvas shows nothing.", systemImage: CanvasSymbol.canvas)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .animation(theme.motion.fade, value: current?.key)
                .animation(theme.motion.fade, value: shelf)
            }
            .background(theme.colors.surface)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("canvas.done")
                }
                if let session, session.canvas != nil {
                    ToolbarItem(placement: .principal) {
                        CanvasDock(session: session, store: store, chosen: current?.section, identifier: "canvas.key") { section in
                            store.show(section, in: session)
                        } open: { _ in
                            openBrowser(of: session)
                        }
                    }
                }
                if let session, let current, session.canControl, case .browser = current {
                    ToolbarItem(placement: .primaryAction) {
                        Button { typing.toggle() } label: {
                            Label("Keyboard", systemImage: typing ? "keyboard.chevron.compact.down" : "keyboard")
                        }
                        .accessibilityIdentifier("canvas.keyboard")
                    }
                }
            }
        }
        .onAppear {
            if frame == nil {
                frame = CanvasFrame()
            }
        }
        .onChange(of: current, initial: true) { _, current in
            if let current {
                store.saw(current, in: sessionId)
            }
        }
        .onChange(of: session?.canvas == nil) { _, gone in
            if gone, !store.opensBrowser(sessionId) {
                dismiss()
            }
        }
    }

    @ViewBuilder
    private func actions(_ item: CanvasItem, in session: RuntimeSession) -> some View {
        if session.canControl, session.agent.isAgent {
            Button(session.askTitle, systemImage: "text.bubble") { ask(about: .item(item), words: nil, in: session) }
        }
        Button("Remove from Canvas", systemImage: "xmark", role: .destructive) { remove([item.key], from: session) }
    }

    private func ask(_ current: CanvasShown, in session: RuntimeSession) -> CanvasAsk? {
        guard session.canControl, session.agent.isAgent else { return nil }
        return CanvasAsk(title: session.askTitle, agent: session.agent) { words in
            ask(about: current, words: words, in: session)
        }
    }

    private func ask(about shown: CanvasShown, words: String?, in session: RuntimeSession) {
        Task {
            do {
                try await model.terminals.mutate("session.input", session: session,
                                                 fields: ["text": .string(CanvasQuestion.about(shown, words: words))])
                dismiss()
            } catch { model.notices.show(error) }
        }
    }

    private func openBrowser(of session: RuntimeSession) {
        Task {
            do {
                try await store.openBrowser(in: session.id)
            } catch { model.notices.show(error) }
        }
    }

    private func remove(_ keys: [String], from session: RuntimeSession) {
        Task {
            do {
                try await store.remove(keys, in: session.id)
            } catch { model.notices.show(error) }
        }
    }
}
