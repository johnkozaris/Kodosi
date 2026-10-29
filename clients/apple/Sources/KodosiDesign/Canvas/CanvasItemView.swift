import KodosiBridge
import SwiftUI

#if os(iOS)
    private let control: CGFloat = 40
#else
    private let control: CGFloat = 26
#endif

/// The shared browser of a terminal: the bar, and the picture that people click and type in.
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

    public var body: some View {
        VStack(spacing: 0) {
            bar
            ZStack {
                theme.colors.terminal
                if let image = feed.image, browser.state == .awake {
                    Image(decorative: image, scale: 1).resizable().interpolation(.high).scaledToFit()
                }
                if browser.state == .awake {
                    BrowserInputArea(feed: feed, typing: typing)
                } else {
                    state
                }
            }
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
        .onChange(of: browser.activeTab?.url, initial: true) { _, url in
            if !editing {
                address = url ?? ""
            }
        }
    }

    private var bar: some View {
        HStack(spacing: 4) {
            IconButton(title: "Back", symbol: "chevron.left", identifier: "canvas.browser.back", size: control) { act(.back) }
            IconButton(title: "Forward", symbol: "chevron.right", identifier: "canvas.browser.forward", size: control) { act(.forward) }
            IconButton(title: "Reload", symbol: "arrow.clockwise", identifier: "canvas.browser.reload", size: control) { act(.reload) }
            HStack(spacing: 6) {
                Image(systemName: "globe").font(.system(size: 10, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                TextField(String(localized: "Address", bundle: .module), text: $address)
                    .textFieldStyle(.plain).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                    .focused($editing)
                    .onSubmit { open() }
                    .accessibilityIdentifier("canvas.browser.address")
            }
            .padding(.horizontal, 10).frame(height: control).frame(maxWidth: 560).wellCapsule()
            .layoutPriority(1)
            tabs
            if let failure = feed.failure ?? browser.problem {
                Text(failure).appTextStyle(.caption).foregroundStyle(theme.colors.caution).lineLimit(1).help(failure)
            } else if let last = browser.last {
                HStack(spacing: 5) {
                    AgentMark(kind: AgentKind(agentName: last.agent), size: 14)
                    Text(verbatim: "\(last.agent) · \(last.words)").lineLimit(1).truncationMode(.middle)
                }
                .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                .frame(minWidth: 80, alignment: .leading)
                .help("\(last.agent) · \(last.words)")
            }
        }
        .disabled(browser.state != .awake)
        .padding(.horizontal, 6).frame(height: control + 10)
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
                Text("\(browser.tabs.count) tabs", bundle: .module).appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            }
            .menuStyle(.button).buttonStyle(.plain).fixedSize()
            .accessibilityIdentifier("canvas.browser.tabs")
        } else {
            IconButton(title: "New tab", symbol: "plus", identifier: "canvas.browser.newTab", size: control) { act(.newTab) }
        }
    }

    @ViewBuilder
    private var state: some View {
        switch browser.state {
        case .starting:
            TerminalStateCard(symbol: nil, title: String(localized: "Starting the browser", bundle: .module), message: nil,
                              working: true) { EmptyView() }
        case .asleep:
            TerminalStateCard(symbol: "moon.zzz", title: String(localized: "The browser sleeps", bundle: .module),
                              message: browser.problem, working: false)
            {
                Button { Task { await feed.wake() } } label: { Text("Wake it", bundle: .module) }
                    .buttonStyle(.kodosi(.secondary))
                    .accessibilityIdentifier("canvas.browser.wake")
            }
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
    @State private var content: Content?
    @State private var failure: String?

    private struct Content: Equatable {
        let key: String
        let revision: UInt64
        let data: Data
    }

    public init(session: RuntimeSession, shown: CanvasShown, store: CanvasStore, frame: CanvasFrame, typing: Bool = false) {
        self.session = session
        self.shown = shown
        self.store = store
        self.frame = frame
        self.typing = typing
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
            case .page: CanvasPageView(frame: frame, content: content.data, otherComputer: session.otherComputer)
            case .picture, .pdf, .office: CanvasFileView(content: content.data, name: item.name)
            }
        } else if item.missing {
            TerminalStateCard(symbol: "questionmark.folder", title: String(localized: "The file is missing", bundle: .module),
                              message: item.key, working: false) { EmptyView() }
        } else if let failure {
            TerminalStateCard(symbol: "exclamationmark.triangle", title: String(localized: "This did not load", bundle: .module),
                              message: failure, working: false)
            {
                Button { Task { await load(item) } } label: { Text("Try again", bundle: .module) }.buttonStyle(.kodosi(.secondary))
            }
        } else {
            TerminalStateCard(symbol: nil, title: String(localized: "Loading", bundle: .module), message: nil, working: true) { EmptyView() }
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
            content = Content(key: item.key, revision: revision, data: data)
        } catch is CancellationError {
            return
        } catch {
            failure = error.localizedDescription
        }
    }
}
