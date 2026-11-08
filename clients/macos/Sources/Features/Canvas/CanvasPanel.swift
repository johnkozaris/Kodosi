import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

/// Where the canvases of the shown terminals open: the space in which their keys move between
/// a terminal's header and its canvas, the terminal whose canvas is open, and how to open one.
struct CanvasPlace {
    var flight: Namespace.ID?
    var open: String?
    var show: @MainActor (RuntimeSession, CanvasSection?) -> Void = { _, _ in }
}

extension EnvironmentValues {
    @Entry var canvasPlace = CanvasPlace()
}

/// The widths of a terminal and its canvas side by side, in one body.
struct CanvasPair {
    static let narrowest: CGFloat = 380

    let panel: CGFloat
    let narrow: Bool

    init(width: CGFloat, chosen: CGFloat?) {
        narrow = width < Self.narrowest * 2
        let most = max(Self.narrowest, width - Self.narrowest)
        panel = min(max(chosen ?? width / 2, Self.narrowest), most)
    }

    /// How much a terminal of `tile` zooms out to wait in the corner of a stage of `stage`.
    static func zoom(tile: CGSize, stage: CGSize) -> CGFloat {
        guard tile.width > 0, tile.height > 0 else { return 1 }
        return min(0.45, 320 / tile.width, stage.width * 0.28 / tile.width, stage.height * 0.4 / tile.height)
    }
}

/// A terminal with its canvas open: two lobes of one body with a soft waist between them, or
/// the canvas over all the stage with the terminal live and zoomed out in its corner. The
/// terminal keeps its size in both. With `always`, the body is there without a canvas too, and
/// `tab` is the tab that it flows out of. A canvas that opens beside `front`, the terminal in
/// front, unfolds from it. One that hides, or comes and goes with another terminal in front,
/// does so at once, as the terminal does.
struct CanvasStage<Terminal: View>: View {
    static var corner: CGFloat {
        16
    }

    @Environment(\.theme) private var theme
    @Environment(\.tileRadius) private var radius
    let canvas: RuntimeSession?
    let front: String?
    @Binding var wide: Bool
    @Binding var width: CGFloat?
    var tab: CGRect?
    var always = false
    let hide: () -> Void
    let terminal: Terminal
    @State private var shown: String?
    @State private var shownFront: String?

    init(canvas: RuntimeSession?, front: String?, wide: Binding<Bool>, width: Binding<CGFloat?>, tab: CGRect? = nil,
         always: Bool = false, hide: @escaping () -> Void, @ViewBuilder terminal: () -> Terminal)
    {
        self.canvas = canvas
        self.front = front
        _wide = wide
        _width = width
        self.tab = tab
        self.always = always
        self.hide = hide
        self.terminal = terminal()
    }

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let pair = CanvasPair(width: size.width, chosen: width)
            let open = canvas != nil
            let beside = open && !pair.narrow
            let zoomed = open && (wide || pair.narrow)
            let tile = CGSize(width: beside ? size.width - pair.panel : size.width, height: size.height)
            let zoom = CanvasPair.zoom(tile: tile, stage: size)
            let seam = zoomed ? 0 : size.width - pair.panel
            ZStack(alignment: .trailing) {
                if open || always {
                    CanvasBody(seam: seam, depth: beside && !zoomed ? CanvasBody.waist : 0, radius: radius, tab: tab)
                        .transition(.modifier(active: Grow(part: tile.width / max(size.width, 1)), identity: Grow(part: 1)))
                }
                terminal
                    .environment(\.terminalJoined, open || always)
                    .transformEnvironment(\.terminalInteractionEnabled) { $0 = $0 && !zoomed }
                    .modifier(ZoomedOut(part: zoomed ? 1 : 0, zoom: zoom, radius: radius,
                                        offset: CGSize(width: size.width - Self.corner - tile.width, height: -Self.corner),
                                        name: canvas?.name ?? "") { wide = false })
                    .padding(.trailing, beside ? pair.panel : 0)
                    .transaction(value: canvas?.id) { $0.animation = nil }
                    .zIndex(open ? 2 : 0)
                if let canvas {
                    CanvasPanel(session: canvas, wide: zoomed, widen: { wide = $0 }, hide: hide)
                        .frame(width: zoomed ? nil : pair.panel)
                        .id(canvas.id)
                        .transition(.asymmetric(insertion: theme.motion.reduced ? .opacity : .unfold, removal: .identity))
                        .zIndex(1)
                }
                if beside, !zoomed {
                    CanvasHandle(width: $width, current: pair.panel, widest: max(CanvasPair.narrowest, size.width - CanvasPair.narrowest)) {
                        wide = true
                    }
                    .padding(.trailing, pair.panel - CanvasHandle.reach / 2)
                    .zIndex(3)
                }
            }
        }
        .animation(opens ? theme.motion.soft : nil, value: canvas?.id)
        .animation(theme.motion.soft, value: wide)
        .onChange(of: canvas?.id, initial: true) { _, id in shown = id }
        .onChange(of: front, initial: true) { _, id in shownFront = id }
        .onChange(of: wide) { _, wide in
            if wide {
                NSApp.keyWindow?.makeFirstResponder(nil)
            }
        }
    }

    /// Whether a canvas opens now beside the terminal that was in front without one.
    private var opens: Bool {
        canvas != nil && shown == nil && (shownFront == nil || shownFront == front)
    }
}

/// The canvas of a terminal, beside it: the keys of its parts and the part that it shows.
struct CanvasPanel: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.canvasPlace) private var place
    @Environment(\.tileRadius) private var radius
    let session: RuntimeSession
    let wide: Bool
    let widen: (Bool) -> Void
    let hide: () -> Void
    @State private var frames = CanvasFrames()
    @State private var hovered = false

    var body: some View {
        let current = deps.canvas.current(session)
        let shelf = current?.section == .files && deps.canvas.showsShelf(session.id)
        VStack(spacing: 0) {
            header
            VStack(spacing: 0) {
                if current?.section == .files {
                    CanvasFilesBar(session: session, store: deps.canvas, chosen: current?.key) { key in
                        deps.canvas.pick(key, in: session.id)
                    } remove: { item in
                        remove([item.key])
                    } actions: { item in
                        actions(item)
                    }
                    Rectangle().fill(theme.colors.hairline.opacity(0.6)).frame(height: 0.5)
                }
                ZStack {
                    if shelf {
                        CanvasShelf(session: session, store: deps.canvas) { key in
                            deps.canvas.pick(key, in: session.id)
                        } remove: { item in
                            remove([item.key])
                        } actions: { item in
                            actions(item)
                        }
                    } else if let current {
                        CanvasItemView(session: session, shown: current, store: deps.canvas, frames: frames, pages: deps.documentPages,
                                       ask: ask(current))
                            .id(current.key)
                    }
                }
                .transaction(value: current?.key) { $0.animation = nil }
                .transaction(value: shelf) { $0.animation = nil }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .overlay(alignment: .top) { progress(current) }
                .dropDestination(for: URL.self) { files, _ in
                    deps.add(files, to: session)
                    return !files.isEmpty
                }
            }
            .background(CanvasContentGround())
            .clipShape(RoundedRectangle(cornerRadius: max(Radius.sm, radius - 5), style: .continuous))
            .chromeScope()
            .padding(.top, 3).padding([.horizontal, .bottom], 5)
        }
        .terminalScope()
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .onChange(of: current, initial: true) { _, current in
            if let current {
                deps.canvas.saw(current, in: session.id)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("canvas.\(AccessibilityIdentifier.token(session.id))")
    }

    private var header: some View {
        HStack(spacing: 8) {
            CanvasDock(session: session, store: deps.canvas, chosen: deps.canvas.current(session)?.section, flight: place.flight,
                       identifier: "canvas.key")
            { section in
                deps.canvas.show(section, in: session)
            } close: { section in
                deps.canvas.close(section, in: session)
                if deps.canvas.sections(session).isEmpty {
                    hide()
                }
            } open: { section in
                if section == .browser {
                    deps.openBrowser(of: session)
                } else {
                    deps.canvas.show(section, in: session)
                }
            } add: {
                deps.addFile(to: session)
            }
            Color.clear.frame(minWidth: 6, maxWidth: .infinity).titleBarGap()
            HStack(spacing: 0) {
                IconButton(title: wide ? "Side by side" : "More room",
                           symbol: wide ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right",
                           identifier: "canvas.wide", size: 26)
                {
                    widen(!wide)
                }
                IconButton(title: "Hide canvas", symbol: "sidebar.trailing", identifier: "canvas.hide", size: 26, action: hide)
            }
            .opacity(hovered ? 1 : 0.28)
        }
        .padding(.leading, wide ? 7 : 7 + CanvasBody.clearance).padding(.trailing, 6).frame(height: CanvasBody.band)
    }

    @ViewBuilder
    private func actions(_ item: CanvasItem) -> some View {
        if !item.missing {
            Button("Save a Copy…", systemImage: "square.and.arrow.down") { save(item) }
        }
        if session.canControl, AgentKind(program: session.program).isAgent {
            Button(session.askTitle, systemImage: "text.bubble") { ask(about: .item(item), words: nil) }
        }
        Divider()
        Button("Remove from Canvas", systemImage: "xmark") { remove([item.key]) }
    }

    @ViewBuilder
    private func progress(_ current: CanvasShown?) -> some View {
        if let current, let part = deps.canvas.read(current.key, in: session.id) {
            GeometryReader { geometry in
                Capsule().fill(theme.colors.accent)
                    .frame(width: max(6, geometry.size.width * part), height: 2)
                    .animation(theme.motion.snappy, value: part)
            }
            .frame(height: 2)
            .transition(.opacity)
            .accessibilityHidden(true)
        }
    }

    private func ask(_ current: CanvasShown?) -> CanvasAsk? {
        guard let current, session.canControl, AgentKind(program: session.program).isAgent else { return nil }
        return CanvasAsk(title: session.askTitle, agent: AgentKind(program: session.program)) { words in
            ask(about: current, words: words)
        }
    }

    private func ask(about shown: CanvasShown, words: String?) {
        Task { @MainActor in
            let words: String? = if let words {
                words
            } else if case let .item(item) = shown {
                await selectedWords(of: item)
            } else {
                nil
            }
            do {
                try await deps.mutateSession("session.input", session: session,
                                             fields: ["text": .string(CanvasQuestion.about(shown, words: words))])
                widen(false)
                deps.workbench.selectSession(session.id)
            } catch { deps.errorMessage = error.localizedDescription }
        }
    }

    /// The words that the person selected in what shows of an item: its page or its live document.
    private func selectedWords(of item: CanvasItem) async -> String? {
        if let words = await frames.selectedWords(of: item.key) {
            return words
        }
        return await deps.documentPages.selectedWords(of: item.key, in: session.id)
    }

    private func remove(_ keys: [String]) {
        Task { @MainActor in
            do {
                try await deps.canvas.remove(keys, in: session.id)
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

extension AppDependencies {
    /// Shows the shared browser of a terminal on its canvas, and opens it when the terminal has none.
    func openBrowser(of session: RuntimeSession) {
        Task { @MainActor in
            do {
                try await canvas.openBrowser(of: session)
            } catch { errorMessage = error.localizedDescription }
        }
    }

    /// Lets a person pick files for the canvas of a terminal.
    func addFile(to session: RuntimeSession) {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK else { return }
        add(panel.urls, to: session)
    }

    func add(_ files: [URL], to session: RuntimeSession) {
        workbench.openCanvas(session.id)
        Task { @MainActor in
            do {
                for file in files {
                    try await canvas.attach(file, in: session.id)
                }
            } catch { errorMessage = error.localizedDescription }
        }
    }
}

/// Shows the leading part of a view, from its leading edge.
private struct Grow: ViewModifier, Animatable {
    var part: Double

    var animatableData: Double {
        get { part }
        set { part = newValue }
    }

    func body(content: Content) -> some View {
        GeometryReader { geometry in
            content.frame(width: geometry.size.width * min(1, max(0, part)))
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
    }
}

/// The terminal of a canvas that fills the stage: live and zoomed out in the stage's corner. A
/// press shows them side by side again.
private struct ZoomedOut: ViewModifier, Animatable {
    @Environment(\.theme) private var theme
    var part: Double
    let zoom: CGFloat
    let radius: CGFloat
    let offset: CGSize
    let name: String
    let grow: () -> Void
    @State private var hovered = false

    var animatableData: Double {
        get { part }
        set { part = newValue }
    }

    func body(content: Content) -> some View {
        let away = part > 0.001
        let lift = hovered && part > 0.999 ? 1.035 : 1
        content
            .allowsHitTesting(!away)
            .accessibilityHidden(away)
            .background {
                if away {
                    CanvasBody(seam: .greatestFiniteMagnitude, depth: 0, radius: radius)
                }
            }
            .overlay {
                if away {
                    Button(action: grow) { Color.clear.contentShape(RoundedRectangle(cornerRadius: radius)) }
                        .buttonStyle(.plain)
                        .onHover { hovered = $0 }
                        .help("Side by side")
                        .accessibilityLabel(Text(name))
                        .accessibilityHint(Text("Shows the terminal beside its canvas"))
                        .accessibilityIdentifier("canvas.terminal")
                }
            }
            .scaleEffect((1 - (1 - zoom) * part) * lift, anchor: .bottomTrailing)
            .offset(x: offset.width * part, y: offset.height * part)
            .animation(theme.motion.hover, value: hovered)
    }
}

extension AnyTransition {
    /// The canvas pulls out of its terminal's edge: it shows from its leading edge to its
    /// trailing edge, at its own size.
    static var unfold: AnyTransition {
        .modifier(active: Unfold(shown: 0), identity: Unfold(shown: 1))
    }
}

private struct Unfold: ViewModifier, Animatable {
    var shown: Double

    var animatableData: Double {
        get { shown }
        set { shown = newValue }
    }

    func body(content: Content) -> some View {
        content.mask(alignment: .leading) {
            GeometryReader { geometry in
                RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
                    .frame(width: geometry.size.width * max(0, shown))
            }
        }
    }
}

/// The ground of what a canvas shows, in the appearance of the app.
private struct CanvasContentGround: View {
    @Environment(\.theme) private var theme

    var body: some View {
        theme.colors.sheet
    }
}

/// The gap between a terminal and its canvas, which a person drags to share the width.
struct CanvasHandle: View {
    static let reach: CGFloat = 12

    @Environment(\.theme) private var theme
    @Binding var width: CGFloat?
    let current: CGFloat
    let widest: CGFloat
    let widen: () -> Void
    @State private var start: CGFloat?
    @State private var hovered = false

    var body: some View {
        Capsule().fill(theme.colors.inkFaint.opacity(hovered || start != nil ? 0.7 : 0))
            .frame(width: 3, height: 34)
            .frame(width: Self.reach).frame(maxHeight: .infinity)
            .padding(.top, CanvasBody.band)
            .contentShape(Rectangle())
            .pointerStyle(.columnResize)
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
            .gesture(DragGesture(minimumDistance: 1, coordinateSpace: .global).onChanged { value in
                let from = start ?? current
                start = from
                width = min(max(from - value.translation.width, CanvasPair.narrowest), widest)
            }.onEnded { _ in start = nil })
            .onTapGesture(count: 2, perform: widen)
            .terminalScope()
            .accessibilityHidden(true)
    }
}
