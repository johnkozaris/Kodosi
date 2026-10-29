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

/// The items of a terminal's canvas. A tap opens one full screen.
struct CanvasSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    let open: (String) -> Void

    var body: some View {
        NavigationStack {
            Group {
                if let session = model.terminals.session(sessionId), let canvas = session.canvas {
                    List(canvas.shown) { shown in
                        Button { open(shown.key) } label: { row(shown, in: session) }
                            .swipeActions { Button("Remove", role: .destructive) { remove(shown, from: session) } }
                            .accessibilityIdentifier("canvas.item.\(AccessibilityIdentifier.token(shown.key))")
                    }
                } else {
                    ContentUnavailableView("The canvas shows nothing.", systemImage: CanvasSymbol.canvas)
                }
            }
            .navigationTitle("Canvas")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("canvas.done")
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationBackground(theme.colors.surface)
    }

    private func row(_ shown: CanvasShown, in session: RuntimeSession) -> some View {
        HStack(spacing: 12) {
            Image(systemName: shown.symbol).font(.system(size: 17, weight: .medium))
                .foregroundStyle(theme.colors.accentStrong).frame(width: 36, height: 36).well(Radius.md)
            VStack(alignment: .leading, spacing: 2) {
                Text(shown.title).appTextStyle(.body).foregroundStyle(shown.missing ? theme.colors.inkFaint : theme.colors.ink)
                    .lineLimit(1).truncationMode(.middle)
                Text(detail(shown)).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
            }
            Spacer(minLength: 8)
            if model.terminals.canvas.isNew(shown, in: session.id) {
                BreathingDot(color: theme.colors.glowOrange, size: 7)
            }
        }
        .contentShape(Rectangle())
    }

    private func detail(_ shown: CanvasShown) -> String {
        switch shown {
        case let .item(item) where item.missing: String(localized: "Missing")
        case let .item(item): item.kind.label
        case let .browser(browser): browser.activeTab?.url ?? String(localized: "Shared browser")
        }
    }

    private func remove(_ shown: CanvasShown, from session: RuntimeSession) {
        Task {
            do {
                try await model.terminals.canvas.remove([shown.key], in: session.id)
            } catch { model.notices.show(error) }
        }
    }
}

/// One item of a canvas full screen, with the other items as chips above it.
struct CanvasScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let sessionId: String
    @State private var frame: CanvasFrame?
    @State private var typing = false

    var body: some View {
        let session = model.terminals.session(sessionId)
        let current = session.flatMap(model.terminals.canvas.current)
        NavigationStack {
            Group {
                if let session, let current, let frame {
                    CanvasItemView(session: session, shown: current, store: model.terminals.canvas, frame: frame, typing: typing)
                        .id(current.key)
                } else if session?.canvas == nil {
                    ContentUnavailableView("The canvas shows nothing.", systemImage: CanvasSymbol.canvas)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(theme.colors.surface)
            .safeAreaInset(edge: .top, spacing: 0) {
                if let session {
                    chips(session, current: current)
                }
            }
            .navigationTitle(current?.title ?? String(localized: "Canvas"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("canvas.done")
                }
                if let session, let current, session.canControl {
                    ToolbarItemGroup(placement: .primaryAction) {
                        if case .browser = current {
                            Button { typing.toggle() } label: {
                                Label("Keyboard", systemImage: typing ? "keyboard.chevron.compact.down" : "keyboard")
                            }
                            .accessibilityIdentifier("canvas.keyboard")
                        }
                        Button { ask(about: current, in: session) } label: { Label("Ask about this", systemImage: "text.bubble") }
                            .accessibilityIdentifier("canvas.ask")
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
                model.terminals.canvas.saw(current, in: sessionId)
            }
        }
        .onChange(of: session?.canvas == nil) { _, gone in
            if gone {
                dismiss()
            }
        }
    }

    private func chips(_ session: RuntimeSession, current: CanvasShown?) -> some View {
        ScrollViewReader { chips in
            ScrollView(.horizontal) {
                HStack(spacing: 6) {
                    ForEach(session.canvas?.shown ?? []) { shown in
                        let selected = shown.key == current?.key
                        Button { model.terminals.canvas.pick(shown.key, in: session.id) } label: {
                            HStack(spacing: 6) {
                                Image(systemName: shown.symbol).font(.system(size: 12, weight: .semibold))
                                Text(shown.title).lineLimit(1).truncationMode(.middle)
                            }
                            .appTextStyle(.footnote).fontWeight(.medium)
                            .foregroundStyle(shown.missing ? theme.colors.inkFaint : selected ? theme.colors.ink : theme.colors.inkMuted)
                            .padding(.horizontal, 12).frame(minHeight: 36).frame(maxWidth: 220)
                            .background {
                                if selected {
                                    RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                                }
                            }
                            .overlay(alignment: .topTrailing) {
                                if !selected, model.terminals.canvas.isNew(shown, in: session.id) {
                                    BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -2, y: 2)
                                }
                            }
                            .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(selected ? .isSelected : [])
                        .accessibilityIdentifier("canvas.chip.\(AccessibilityIdentifier.token(shown.key))")
                    }
                }
                .padding(.horizontal, 12).padding(.vertical, 6)
            }
            .scrollIndicators(.never)
            .onChange(of: current?.key, initial: true) { _, key in
                if let key {
                    withAnimation(theme.motion.snappy) { chips.scrollTo(key) }
                }
            }
        }
        .background(theme.colors.surface)
    }

    private func ask(about shown: CanvasShown, in session: RuntimeSession) {
        Task {
            let words: String? = if case let .item(item) = shown, item.kind == .page {
                await frame?.selection()
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
}

extension CanvasItem.Kind {
    var label: String {
        switch self {
        case .page: String(localized: "Page")
        case .picture: String(localized: "Picture")
        case .pdf: String(localized: "PDF")
        case .office: String(localized: "Office file")
        }
    }
}
