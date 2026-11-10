import KodosiBridge
import KodosiDesign
import SwiftUI
import UniformTypeIdentifiers

/// The canvas key in a terminal's bar: there when the canvas shows something, in a well, with a
/// breathing dot for news and a hop when the news comes.
struct CanvasKey: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let canvas: CanvasStore
    let open: () -> Void

    var body: some View {
        let news = canvas.hasNews(session)
        Button(action: open) {
            Image(systemName: CanvasSymbol.canvas)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.colors.inkMuted)
                .hop(when: news)
                .frame(width: 36, height: 28)
                .overlay(alignment: .topTrailing) {
                    if news {
                        BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -3, y: 3).transition(AnyTransition.pop)
                    }
                }
                .wellCapsule()
                .touchTarget()
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityLabel(Text("Canvas"))
        .accessibilityValue(news ? Text("Something new") : Text(verbatim: ""))
        .accessibilityIdentifier("terminal.canvas")
    }
}

private struct CanvasCopy {
    let name: String
    let data: Data
}

/// Lets a person pick files for the canvas of a terminal.
struct CanvasFilePicker: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var isPresented: Bool
    let session: RuntimeSession?
    let added: () -> Void

    func body(content: Content) -> some View {
        content.fileImporter(isPresented: $isPresented, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard let session, case let .success(files) = result else { return }
            Task {
                do {
                    for file in files {
                        try await model.terminals.canvas.attach(file, in: session.id)
                    }
                    added()
                } catch { model.notices.show(error) }
            }
        }
    }
}

/// The canvas of a terminal full screen: its parts in the bar, the files as tabs or cards, and
/// the part that it shows.
struct CanvasScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    @State private var frames = CanvasFrames()
    @State private var typing = false
    @State private var addingFile = false
    @State private var copy: CanvasCopy?

    private var store: CanvasStore {
        model.terminals.canvas
    }

    var body: some View {
        let session = model.terminals.session(sessionId)
        let current = session.flatMap(store.current)
        let shelf = current?.section == .files && store.showsShelf(sessionId)
        let shows = session.map(store.showsSomething) == true || store.opensBrowser(sessionId)
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
                    } else if let session, let current {
                        CanvasItemView(session: session, shown: current, store: store, frames: frames, pages: model.terminals.documentPages,
                                       typing: typing && !model.lock.locked, ask: ask(current, in: session))
                            .id(current.key)
                    } else if store.opensBrowser(sessionId) {
                        Spinner(size: 22)
                    } else if session?.canvas == nil {
                        EmptyState(title: "The canvas shows nothing.") {
                            IconTile(symbol: CanvasSymbol.canvas, tint: TileTint.graphite, size: 52)
                        } actions: {
                            EmptyView()
                        }
                    }
                }
                .transaction(value: current?.key) { $0.animation = nil }
                .transaction(value: shelf) { $0.animation = nil }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .background(theme.colors.sheet)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Done", symbol: "xmark", identifier: "canvas.done", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
                if let session, session.canvas != nil {
                    ToolbarItem(placement: .principal) {
                        CanvasDock(session: session, store: store, chosen: current?.section, identifier: "canvas.key") { section in
                            store.show(section, in: session)
                        } close: { section in
                            store.close(section, in: session)
                        } open: { section in
                            if section == .browser {
                                openBrowser(of: session)
                            } else {
                                store.show(section, in: session)
                            }
                        } add: {
                            addingFile = true
                        }
                    }
                    .sharedBackgroundVisibility(.hidden)
                }
                if let session, let current, session.canControl, case .browser = current {
                    ToolbarItem(placement: .primaryAction) {
                        IconButton(title: "Keyboard", symbol: typing ? "keyboard.chevron.compact.down" : "keyboard",
                                   identifier: "canvas.keyboard", size: 32, active: typing) { typing.toggle() }
                    }
                    .sharedBackgroundVisibility(.hidden)
                }
            }
        }
        .modifier(CanvasFilePicker(isPresented: $addingFile, session: session) {})
        .fileExporter(isPresented: Binding { copy != nil } set: { _ in copy = nil }, item: copy?.data, contentTypes: [.data],
                      defaultFilename: copy?.name) { _ in copy = nil }
        .onChange(of: current, initial: true) { _, current in
            if let current {
                store.saw(current, in: sessionId)
            }
        }
        .onChange(of: shows) { _, shows in
            if !shows {
                dismiss()
            }
        }
    }

    @ViewBuilder
    private func actions(_ item: CanvasItem, in session: RuntimeSession) -> some View {
        if !item.missing {
            Button("Save a Copy…", systemImage: "square.and.arrow.down") { save(item, from: session) }
        }
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
            let words: String? = if let words {
                words
            } else if case let .item(item) = shown {
                await selectedWords(of: item, in: session)
            } else {
                nil
            }
            do {
                try await model.terminals.mutate("session.input", session: session,
                                                 fields: ["text": .string(CanvasQuestion.about(shown, words: words))])
                dismiss()
            } catch { model.notices.show(error) }
        }
    }

    /// The words that the person selected in what shows of an item: its page or its live document.
    private func selectedWords(of item: CanvasItem, in session: RuntimeSession) async -> String? {
        if let words = await frames.selectedWords(of: item.key) {
            return words
        }
        return await model.terminals.documentPages.selectedWords(of: item.key, in: session.id)
    }

    private func save(_ item: CanvasItem, from session: RuntimeSession) {
        Task {
            do {
                let (_, data) = try await store.content(of: item, in: session.id)
                copy = CanvasCopy(name: item.name, data: data)
            } catch { model.notices.show(error) }
        }
    }

    private func openBrowser(of session: RuntimeSession) {
        Task {
            do {
                try await store.openBrowser(of: session)
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
