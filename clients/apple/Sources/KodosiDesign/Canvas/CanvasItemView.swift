import KodosiBridge
import SwiftUI

#if os(iOS)
    private let control: CGFloat = 40
#else
    private let control: CGFloat = 26
#endif

/// The shared browser of a terminal: the bar, and the picture that people click and type in,
/// with the agent's pointer over it. Asleep, the last picture stays in grey.
public struct CanvasBrowserView: View {
    @Environment(\.theme) private var theme
    let browser: CanvasBrowser
    let feed: BrowserFeed
    var typing = false
    @State private var address = ""
    @FocusState private var editing: Bool

    public init(browser: CanvasBrowser, feed: BrowserFeed, typing: Bool = false) {
        self.browser = browser
        self.feed = feed
        self.typing = typing
    }

    private var blank: Bool {
        (browser.activeTab?.url ?? Self.blankPage) == Self.blankPage
    }

    private static let blankPage = "about:blank"

    public var body: some View {
        VStack(spacing: 0) {
            bar
            ZStack {
                theme.colors.terminal
                if blank, browser.state == .awake {
                    Image(systemName: "globe").font(.system(size: 34, weight: .light))
                        .foregroundStyle(theme.colors.inkFaint.opacity(0.6))
                        .transition(.opacity)
                } else if let image = feed.image {
                    Image(decorative: image, scale: 1).resizable().interpolation(.high).scaledToFit()
                        .grayscale(browser.state == .awake ? 0 : 1)
                        .animation(theme.motion.fade, value: browser.state)
                }
                if browser.state == .awake {
                    BrowserInputArea(feed: feed, typing: typing)
                    AgentPointerLayer(browser: browser, feed: feed)
                } else {
                    sleeping.transition(.opacity)
                }
            }
            .clipped()
            .onGeometryChange(for: CGSize.self) { $0.size } action: { feed.viewSize = $0 }
            .task(id: browser.state) {
                switch browser.state {
                case .awake: await feed.watch()
                case .asleep: if browser.problem == nil {
                        await feed.wake()
                    }
                case .starting: break
                }
            }
        }
        .animation(theme.motion.fade, value: browser.state)
        .onChange(of: browser.activeTab?.url, initial: true) { _, url in
            if !editing {
                address = url == Self.blankPage ? "" : url ?? ""
            }
        }
    }

    private var bar: some View {
        HStack(spacing: 2) {
            HStack(spacing: 2) {
                IconButton(title: "Back", symbol: "chevron.left", identifier: "canvas.browser.back", size: control) { act(.back) }
                IconButton(title: "Forward", symbol: "chevron.right", identifier: "canvas.browser.forward", size: control) { act(.forward) }
                IconButton(title: "Reload", symbol: "arrow.clockwise", identifier: "canvas.browser.reload", size: control) { act(.reload) }
                HStack(spacing: 6) {
                    Image(systemName: secure ? "lock.fill" : "globe").font(.system(size: 9.5, weight: .semibold))
                        .foregroundStyle(theme.colors.inkFaint)
                    TextField(String(localized: "Address", bundle: .module), text: $address)
                        .textFieldStyle(.plain).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                        .focused($editing)
                        .onSubmit { open() }
                        .accessibilityIdentifier("canvas.browser.address")
                }
                .padding(.horizontal, 10).frame(height: control - 2).frame(maxWidth: 620).wellCapsule()
                .padding(.leading, 4)
                .layoutPriority(1)
                tabs
            }
            .disabled(browser.state != .awake)
            if let failure = feed.failure ?? browser.problem {
                Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(theme.colors.caution).help(failure)
                    .accessibilityLabel(Text(failure))
            }
        }
        .padding(.horizontal, 6).frame(height: control + 8)
    }

    private var secure: Bool {
        browser.activeTab?.url.hasPrefix("https://") == true
    }

    @ViewBuilder
    private var tabs: some View {
        if browser.tabs.count > 1 {
            Menu {
                ForEach(browser.tabs) { tab in
                    Button(tab.title.isEmpty ? tab.url : tab.title) { act(.tab(tab.id)) }
                }
                Divider()
                Button(String(localized: "New tab", bundle: .module)) { act(.newTab) }
                if let active = browser.activeTab {
                    Button(String(localized: "Close tab", bundle: .module)) { act(.closeTab(active.id)) }
                }
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "square.on.square").font(.system(size: 10, weight: .semibold))
                    Text(browser.tabs.count, format: .number).monospacedDigit()
                }
                .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                .padding(.horizontal, 8).frame(height: control - 2)
            }
            .menuStyle(.button).buttonStyle(.plain).fixedSize()
            .help(String(localized: "Tabs", bundle: .module))
            .accessibilityIdentifier("canvas.browser.tabs")
        } else {
            IconButton(title: "New tab", symbol: "plus", identifier: "canvas.browser.newTab", size: control) { act(.newTab) }
        }
    }

    @ViewBuilder
    private var sleeping: some View {
        switch browser.state {
        case .starting:
            Spinner(size: 18)
        case .asleep:
            Button { Task { await feed.wake() } } label: {
                HStack(spacing: 7) {
                    Image(systemName: "moon.zzz.fill").font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(theme.colors.inkMuted)
                    Text("Wake", bundle: .module).appTextStyle(.footnote).fontWeight(.semibold).foregroundStyle(theme.colors.ink)
                }
                .padding(.horizontal, 14).frame(height: 32)
                .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
                .contentShape(Capsule())
            }
            .buttonStyle(PressScaleStyle())
            .help(browser.problem ?? String(localized: "The browser sleeps. It opens its last page again.", bundle: .module))
            .accessibilityIdentifier("canvas.browser.wake")
        case .awake:
            EmptyView()
        }
    }

    private func open() {
        let typed = address.trimmingCharacters(in: .whitespaces)
        guard !typed.isEmpty else { return }
        let url = typed.contains("://") || typed.hasPrefix("about:") ? typed : "https://" + typed
        editing = false
        act(.open(url))
    }

    private func act(_ act: BrowserAct) {
        Task { await feed.run(act) }
    }
}

/// The item of a canvas that a device shows: a page, a file or the browser.
public struct CanvasItemView: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let shown: CanvasShown
    let store: CanvasStore
    let frame: CanvasFrame
    var typing = false
    var ask: CanvasAsk?
    @State private var content: Content?
    @State private var failure: String?

    private struct Content: Equatable {
        let key: String
        let revision: UInt64
        let data: Data
    }

    public init(session: RuntimeSession, shown: CanvasShown, store: CanvasStore, frame: CanvasFrame, typing: Bool = false,
                ask: CanvasAsk? = nil)
    {
        self.session = session
        self.shown = shown
        self.store = store
        self.frame = frame
        self.typing = typing
        self.ask = ask
    }

    public var body: some View {
        switch shown {
        case let .browser(browser):
            CanvasBrowserView(browser: browser, feed: store.feed(session.id), typing: typing)
        case let .item(item):
            file(item)
                .task(id: Revision(key: item.key, revision: item.revision, missing: item.missing)) { await load(item) }
        }
    }

    private struct Revision: Equatable {
        let key: String
        let revision: UInt64
        let missing: Bool
    }

    @ViewBuilder
    private func file(_ item: CanvasItem) -> some View {
        if let content, content.key == item.key {
            switch item.kind {
            case .page: CanvasPageView(frame: frame, content: content.data, otherComputer: session.otherComputer, ask: ask)
            case .picture, .pdf, .office: CanvasFileView(content: content.data, name: item.name).transition(.opacity)
            }
        } else if item.missing {
            CanvasMissing(item: item)
        } else if let failure {
            TerminalStateCard(symbol: "exclamationmark.triangle", title: String(localized: "This did not load", bundle: .module),
                              message: failure, working: false)
            {
                Button { Task { await load(item) } } label: { Text("Try again", bundle: .module) }.buttonStyle(.kodosi(.secondary))
            }
        } else {
            CanvasLoading(kind: item.kind)
        }
    }

    private func load(_ item: CanvasItem) async {
        guard !item.missing else { return }
        if content?.key != item.key {
            content = nil
        }
        failure = nil
        do {
            let (revision, data) = try await store.content(of: item, in: session.id)
            guard !Task.isCancelled else { return }
            withAnimation(theme.motion.fade) { content = Content(key: item.key, revision: revision, data: data) }
        } catch is CancellationError {
            return
        } catch {
            failure = error.localizedDescription
        }
    }
}

/// The shape of what comes, while it comes.
struct CanvasLoading: View {
    @Environment(\.theme) private var theme
    let kind: CanvasItem.Kind

    var body: some View {
        Group {
            if kind == .page {
                VStack(alignment: .leading, spacing: 11) {
                    bar(0.52, 17)
                    bar(0.3, 9).padding(.bottom, 12)
                    bar(0.94, 8)
                    bar(0.88, 8)
                    bar(0.7, 8).padding(.bottom, 8)
                    RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(theme.colors.well).frame(height: 84)
                        .padding(.bottom, 8)
                    bar(0.9, 8)
                    bar(0.6, 8)
                }
                .padding(.horizontal, 32).padding(.top, 34)
                .frame(maxWidth: 660, maxHeight: .infinity, alignment: .topLeading)
            } else {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous).fill(theme.colors.well)
                    .aspectRatio(kind == .picture ? 4 / 3 : 3 / 4, contentMode: .fit)
                    .overlay {
                        Image(systemName: kind.symbol)
                            .font(.system(size: 26, weight: .medium)).foregroundStyle(theme.colors.inkFaint.opacity(0.6))
                    }
                    .padding(36)
            }
        }
        .shimmer()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement()
        .accessibilityLabel(Text("Loading", bundle: .module))
    }

    private func bar(_ part: CGFloat, _ height: CGFloat) -> some View {
        GeometryReader { geometry in
            Capsule().fill(theme.colors.well).frame(width: geometry.size.width * part, height: height)
        }
        .frame(height: height)
    }
}

/// A slot for a file that is not where the agent showed it.
struct CanvasMissing: View {
    @Environment(\.theme) private var theme
    let item: CanvasItem

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: item.kind.symbol).font(.system(size: 26, weight: .medium))
                .foregroundStyle(theme.colors.inkFaint)
            Text(item.name).appTextStyle(.footnote).fontWeight(.medium).foregroundStyle(theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle)
        }
        .padding(.horizontal, 20)
        .frame(width: 240, height: 170)
        .background {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
                .strokeBorder(theme.colors.hairline, style: StrokeStyle(lineWidth: 1.5, dash: [6, 5]))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .help(String(localized: "\(item.key) is not there now", bundle: .module))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("canvas.missing")
    }
}
