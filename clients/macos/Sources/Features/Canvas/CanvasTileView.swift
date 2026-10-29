import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// The canvas of a terminal: the items that its agent shows and its shared browser, next to
/// the terminal on the stage or after it in a room.
struct CanvasTileView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Namespace private var chips
    let session: RuntimeSession
    var roomEmbedded = false
    let close: () -> Void
    @State private var frame: CanvasFrame?
    @State private var hovered = false

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: SessionTileView.radius, style: .continuous)
    }

    var body: some View {
        let current = deps.canvas.current(session)
        VStack(spacing: 0) {
            header(current)
                .background {
                    UnevenRoundedRectangle(topLeadingRadius: SessionTileView.radius, topTrailingRadius: SessionTileView.radius,
                                           style: .continuous)
                        .fill(theme.colors.raised.opacity(0.45))
                }
            Group {
                if let current, let frame {
                    CanvasItemView(session: session, shown: current, store: deps.canvas, frame: frame)
                        .id(current.key)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(theme.colors.surface)
            .clipShape(RoundedRectangle(cornerRadius: Radius.sm, style: .continuous))
            .padding(.top, 3).padding([.horizontal, .bottom], 5)
        }
        .background {
            shape.fill(theme.colors.ground)
                .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.14), radius: 10, x: 3, y: 6)
        }
        .overlay { shape.strokeBorder(theme.colors.hairline.opacity(0.7), lineWidth: 0.5).allowsHitTesting(false) }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .onAppear {
            if frame == nil {
                frame = CanvasFrame()
            }
        }
        .onChange(of: current, initial: true) { _, current in
            if let current {
                deps.canvas.saw(current, in: session.id)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("canvas.\(AccessibilityIdentifier.token(session.id))")
    }

    private func header(_ current: CanvasShown?) -> some View {
        HStack(spacing: 8) {
            Image(systemName: CanvasSymbol.canvas).font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.accentStrong)
            Text("Canvas").appTextStyle(.subhead).foregroundStyle(theme.colors.ink)
            if !roomEmbedded {
                Text(session.name).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
            }
            ScrollViewReader { chips in
                ScrollView(.horizontal) {
                    HStack(spacing: 2) {
                        ForEach(session.canvas?.shown ?? []) { shown in
                            chip(shown, selected: shown.key == current?.key)
                        }
                    }
                    .padding(.vertical, 2)
                }
                .scrollIndicators(.never)
                .onChange(of: current?.key, initial: true) { _, key in
                    if let key {
                        withAnimation(theme.motion.snappy) { chips.scrollTo(key) }
                    }
                }
            }
            .animation(theme.motion.snappy, value: current?.key)
            Spacer(minLength: 4)
            actions(current).opacity(hovered ? 1 : 0.28)
        }
        .padding(.leading, 11).padding(.trailing, 6).frame(height: 38)
    }

    private func chip(_ shown: CanvasShown, selected: Bool) -> some View {
        Button { deps.canvas.pick(shown.key, in: session.id) } label: {
            HStack(spacing: 5) {
                Image(systemName: shown.symbol).font(.system(size: 10, weight: .semibold))
                Text(shown.title).lineLimit(1).truncationMode(.middle)
            }
            .appTextStyle(.footnote).fontWeight(.medium)
            .foregroundStyle(shown.missing ? theme.colors.inkFaint : selected ? theme.colors.ink : theme.colors.inkMuted)
            .padding(.horizontal, 10).frame(height: 26).frame(maxWidth: 200)
            .background {
                if selected {
                    RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                        .matchedGeometryEffect(id: "chip", in: chips)
                }
            }
            .overlay(alignment: .topTrailing) {
                if !selected, deps.canvas.isNew(shown, in: session.id) {
                    BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -2, y: 2).transition(AnyTransition.pop)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help(shown.missing ? String(localized: "Missing") : place(of: shown))
        .contextMenu {
            Button("Remove from canvas") { remove(shown) }
        }
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("canvas.chip.\(AccessibilityIdentifier.token(shown.key))")
    }

    private func actions(_ current: CanvasShown?) -> some View {
        HStack(spacing: 0) {
            if let current, session.canControl {
                let agent = AgentKind(program: session.program)
                IconButton(title: agent.isAgent ? "Ask \(agent.label) about this" : "Ask about this", symbol: "text.bubble",
                           identifier: "canvas.ask", size: 26)
                {
                    ask(about: current)
                }
            }
            if case let .item(item)? = current, !item.missing {
                if session.kind == .local {
                    IconButton(title: "Open in its app", symbol: "arrow.up.forward.app", identifier: "canvas.open", size: 26) {
                        deps.openExternalURL(URL(filePath: item.key))
                    }
                } else {
                    IconButton(title: "Save a copy", symbol: "square.and.arrow.down", identifier: "canvas.save", size: 26) {
                        save(item)
                    }
                }
            }
            IconButton(title: roomEmbedded ? "Show the terminal" : "Close canvas", symbol: "xmark", identifier: "canvas.close",
                       size: 26, action: close)
        }
    }

    private func place(of shown: CanvasShown) -> String {
        switch shown {
        case let .item(item): item.key
        case let .browser(browser): browser.activeTab?.url ?? shown.title
        }
    }

    private func ask(about shown: CanvasShown) {
        Task { @MainActor in
            let words: String? = if case let .item(item) = shown, item.kind == .page {
                await frame?.selection()
            } else {
                nil
            }
            do {
                try await deps.mutateSession("session.input", session: session,
                                             fields: ["text": .string(CanvasQuestion.about(shown, words: words))])
                deps.workbench.selectSession(session.id)
            } catch { deps.errorMessage = error.localizedDescription }
        }
    }

    private func remove(_ shown: CanvasShown) {
        Task { @MainActor in
            do {
                try await deps.canvas.remove([shown.key], in: session.id)
            } catch { deps.errorMessage = error.localizedDescription }
        }
    }

    private func save(_ item: CanvasItem) {
        Task { @MainActor in
            do {
                let (_, data) = try await deps.canvas.content(of: item, in: session.id)
                let panel = NSSavePanel()
                panel.nameFieldStringValue = item.name
                guard panel.runModal() == .OK, let url = panel.url else { return }
                try data.write(to: url, options: .atomic)
            } catch { deps.errorMessage = error.localizedDescription }
        }
    }
}

/// The canvas key in a terminal's header: there when the canvas shows something, with a
/// breathing dot and one hop for news, and raised while the canvas is open.
struct CanvasKey: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let open: Bool
    let identifier: String
    let toggle: () -> Void

    var body: some View {
        let news = !open && deps.canvas.hasNews(session)
        IconButton(title: "Canvas", symbol: CanvasSymbol.canvas, identifier: identifier, size: 26, active: open, action: toggle)
            .overlay(alignment: .topTrailing) {
                if news {
                    BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -3, y: 3)
                        .allowsHitTesting(false).transition(AnyTransition.pop)
                }
            }
            .hop(when: news)
            .animation(theme.motion.snappy, value: news)
    }
}
