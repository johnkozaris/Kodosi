import KodosiBridge
import SwiftUI
import WebKit

/// Hosts the bundled document viewer in a sandboxed WebView that communicates only over the
/// document pipe.
struct CanvasDocumentView: View {
    @Environment(\.theme) private var theme
    @Environment(\.self) private var environment
    @Environment(\.openURL) private var openURL
    @Environment(\.onScreen) private var onScreen
    @AppStorage(DocumentViewers.storageKey) private var viewers = DocumentViewers()
    let session: String
    let item: CanvasItem
    let view: CanvasItem.View
    let store: CanvasStore
    let pages: CanvasDocumentPages
    let otherComputer: String?
    @State private var page: CanvasDocumentPage?

    var body: some View {
        ZStack {
            if let page {
                WebView(page.page)
                    .webViewLinkPreviews(.disabled)
                    .webViewBackForwardNavigationGestures(.disabled)
                    .webViewMagnificationGestures(.disabled)
                    .opacity(page.shown ? 1 : 0)
                    .animation(theme.motion.fade, value: page.shown)
            }
            if page?.shown != true {
                CanvasLoading(kind: item.kind)
            }
        }
        .onAppear {
            if page == nil {
                page = pages.enter(item.key, in: session) {
                    CanvasDocumentPage(link: store.document(item, in: session), view: view, look: look,
                                       preferences: viewers.preferences(for: view))
                }
            }
            page?.links.open = openURL
            page?.links.otherComputer = otherComputer
            page?.restyle(look)
            page?.prefer(viewers.preferences(for: view))
            page?.watch(onScreen)
        }
        .onDisappear {
            if let page {
                page.watch(false)
                pages.leave(page, key: item.key, in: session)
            }
            page = nil
        }
        .onChange(of: onScreen) { _, onScreen in page?.watch(onScreen) }
        .onChange(of: look) { _, look in page?.restyle(look) }
        .onChange(of: viewers.preferences(for: view)) { _, preferences in page?.prefer(preferences) }
    }

    private var look: [String: String] {
        let colors = [
            "surface": theme.colors.surface, "surface-behind": theme.colors.well, "raised": theme.colors.raised,
            "lifted": theme.colors.lifted, "well": theme.colors.well, "ink": theme.colors.ink,
            "ink-soft": theme.colors.inkMuted, "ink-faint": theme.colors.inkFaint, "line": theme.colors.hairline,
            "accent": theme.colors.accentStrong,
        ]
        var look = colors.mapValues { color in
            let color = color.resolve(in: environment)
            let parts = [color.red, color.green, color.blue].map { Int(($0 * 255).rounded()) }
            return String(format: "#%02X%02X%02X", parts[0], parts[1], parts[2])
        }
        look["scheme"] = theme.isDark ? "dark" : "light"
        return look
    }
}

/// Caches off-screen document `WebPage` instances for a while to preserve scroll position and undo
/// history. Each `WebPage` owns a WebKit process, so at most `mostOpen` stay alive (evicting the
/// oldest off-screen page first). Because a `WebPage` can only attach to one view at a time, a
/// concurrent second view of the same file gets a dedicated uncached page.
@MainActor
public final class CanvasDocumentPages {
    private static let kept = Duration.seconds(300)
    private static let mostOpen = 4

    private var pages: [String: CanvasDocumentPage] = [:]
    private var held: Set<String> = []
    private var waiting: [(place: String, closing: Task<Void, Never>)] = []

    public init() {}

    fileprivate func enter(_ key: String, in session: String, open: () -> CanvasDocumentPage) -> CanvasDocumentPage {
        let place = "\(session)/\(key)"
        guard held.insert(place).inserted else { return open() }
        stopWaiting(place)
        if let page = pages[place] {
            return page
        }
        let page = open()
        pages[place] = page
        closeOldest()
        return page
    }

    public func selectedWords(of key: String, in session: String) async -> String? {
        await pages["\(session)/\(key)"]?.selectedWords()
    }

    fileprivate func leave(_ page: CanvasDocumentPage, key: String, in session: String) {
        let place = "\(session)/\(key)"
        guard pages[place] === page else {
            page.close()
            return
        }
        held.remove(place)
        waiting.append((place, Task { [weak self] in
            guard await (try? Task.sleep(for: Self.kept)) != nil else { return }
            self?.close(place)
        }))
        closeOldest()
    }

    private func closeOldest() {
        while pages.count > Self.mostOpen, let oldest = waiting.first {
            close(oldest.place)
        }
    }

    private func stopWaiting(_ place: String) {
        if let index = waiting.firstIndex(where: { $0.place == place }) {
            waiting.remove(at: index).closing.cancel()
        }
    }

    private func close(_ place: String) {
        stopWaiting(place)
        pages.removeValue(forKey: place)?.close()
    }
}

@MainActor
@Observable
final class CanvasDocumentPage {
    nonisolated static let scheme = "kodosi-document"
    nonisolated static let policy = "default-src 'none'; script-src 'self'; worker-src 'self'; style-src 'self' 'unsafe-inline'; "
        + "img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; form-action 'none'; base-uri 'none'"

    @ObservationIgnored let page: WebPage
    @ObservationIgnored let links = CanvasLinks()
    @ObservationIgnored private let link: DocumentLink
    @ObservationIgnored private let messages = PageMessages()
    @ObservationIgnored private let scripts: WKUserContentController
    @ObservationIgnored private let view: CanvasItem.View
    @ObservationIgnored private var look: [String: String]
    @ObservationIgnored private var preferences: [String: Bool]
    @ObservationIgnored private var inbound: AsyncStream<Inbound>.Continuation?
    @ObservationIgnored private var tasks: [Task<Void, Never>] = []
    @ObservationIgnored private var watched = false
    @ObservationIgnored private var seenAgain = false
    @ObservationIgnored private var waking: AsyncStream<Void>.Continuation?
    private(set) var shown = false

    init(link: DocumentLink, view: CanvasItem.View, look: [String: String], preferences: [String: Bool]) {
        self.link = link
        self.view = view
        self.look = look
        self.preferences = preferences
        var configuration = WebPage.Configuration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.urlSchemeHandlers[URLScheme(Self.scheme)!] = ViewBundleHandler()
        configuration.upgradeKnownHostsToHTTPS = false
        scripts = configuration.userContentController
        scripts.add(messages, contentWorld: .page, name: "document")
        scripts.add(messages, contentWorld: .page, name: "ready")
        links.watch(scripts)
        page = WebPage(configuration: configuration, navigationDecider: DocumentNavigation(links: links))
        let (stream, inbound) = AsyncStream.makeStream(of: Inbound.self)
        self.inbound = inbound
        link.onFrame = { inbound.yield(.frame($0)) }
        link.onNews = { inbound.yield(.news($0)) }
        messages.frame = { link.send($0) }
        messages.ready = { [weak self] in self?.shown = true }
        tasks = [
            Task { [weak self] in
                for await item in stream {
                    await self?.take(item)
                }
            },
            Task { [weak self] in await self?.stayOpen() },
        ]
    }

    /// WebKit retains `WKUserContentController` and its script message handlers past `WebPage`
    /// teardown unless explicitly removed.
    isolated deinit {
        scripts.removeAllScriptMessageHandlers()
    }

    func restyle(_ look: [String: String]) {
        guard look != self.look else { return }
        self.look = look
        Task { _ = try? await page.callJavaScript("documentHost.restyle(look)", arguments: ["look": look], contentWorld: .page) }
    }

    func prefer(_ preferences: [String: Bool]) {
        guard preferences != self.preferences else { return }
        self.preferences = preferences
        Task {
            _ = try? await page.callJavaScript("documentHost.prefer(preferences)", arguments: ["preferences": preferences], contentWorld: .page)
        }
    }

    func selectedWords() async -> String? {
        let words = try? await page.callJavaScript("return documentHost.selection()", contentWorld: .page) as? String
        return words?.isEmpty == false ? words : nil
    }

    func close() {
        tasks.forEach { $0.cancel() }
        tasks = []
        inbound?.finish()
        inbound = nil
        link.close()
    }

    func watch(_ watched: Bool) {
        let again = watched && !self.watched
        self.watched = watched
        if again {
            seenAgain = true
            waking?.finish()
        }
    }

    private static let longestRest = Duration.seconds(60)

    /// Reloads and reconnects if the OS terminates the WebKit process (common on iOS).
    private func stayOpen() async {
        let start = URL(string: "\(Self.scheme)://view/index.html")!
        var failures = 0
        while !Task.isCancelled {
            do {
                for try await _ in page.load(URLRequest(url: start)) {}
                _ = try await page.callJavaScript(
                    """
                    window.documentHost = window.documentView.hosted(
                      { kind, theme, preferences, language, ready: () => webkit.messageHandlers.ready.postMessage(true) },
                      (frame) => webkit.messageHandlers.document.postMessage(frame),
                    );
                    """,
                    arguments: [
                        "kind": view.rawValue, "theme": look, "preferences": preferences,
                        "language": AppLanguage.current,
                    ],
                    contentWorld: .page,
                )
            } catch {
                shown = false
                failures += 1
                await rest(after: failures)
                continue
            }
            do {
                try await link.open()
            } catch {
                return
            }
            let opened = ContinuousClock.now
            do {
                for try await _ in page.navigations {}
            } catch {}
            shown = false
            link.close()
            failures = opened.duration(to: .now) < Self.longestRest ? failures + 1 : 1
            await rest(after: failures)
        }
    }

    private func rest(after failures: Int) async {
        if !seenAgain {
            await sleep(atMost: min(Self.longestRest, .seconds(1 << min(failures - 1, 6))))
        }
        while !watched, !Task.isCancelled {
            await sleep(atMost: nil)
        }
        seenAgain = false
    }

    private func sleep(atMost pause: Duration?) async {
        let (wakes, waking) = AsyncStream.makeStream(of: Void.self)
        self.waking = waking
        let alarm = pause.map { pause in
            Task {
                try? await Task.sleep(for: pause)
                waking.finish()
            }
        }
        for await _ in wakes {}
        alarm?.cancel()
        self.waking = nil
    }

    /// Consumed sequentially from one `AsyncStream` so pipe frames and lifecycle events reach JS
    /// in order.
    private func take(_ item: Inbound) async {
        switch item {
        case let .frame(frame):
            let data = frame.base64EncodedString()
            _ = try? await page.callJavaScript("documentHost.deliver(data)", arguments: ["data": data], contentWorld: .page)
        case .news(.open):
            _ = try? await page.callJavaScript("documentHost.attach()", contentWorld: .page)
        case .news(.lost), .news(.closed):
            _ = try? await page.callJavaScript("documentHost.detach()", contentWorld: .page)
        }
    }
}

private enum Inbound {
    case frame(Data)
    case news(DocumentLink.News)
}

@MainActor
private final class PageMessages: NSObject, WKScriptMessageHandler {
    var frame: (Data) -> Void = { _ in }
    var ready: () -> Void = {}

    func userContentController(_: WKUserContentController, didReceive message: WKScriptMessage) {
        if message.name == "ready" {
            ready()
        } else if let text = message.body as? String, let data = Data(base64Encoded: text) {
            frame(data)
        }
    }
}

/// Serves bundled viewer assets only, with no network access.
struct ViewBundleHandler: URLSchemeHandler {
    private static let types = [
        "html": "text/html; charset=utf-8", "js": "text/javascript", "mjs": "text/javascript", "css": "text/css",
        "wasm": "application/wasm", "json": "application/json", "svg": "image/svg+xml", "woff2": "font/woff2",
    ]

    func reply(for request: URLRequest) -> AsyncThrowingStream<URLSchemeTaskResult, any Error> {
        AsyncThrowingStream { continuation in
            guard let url = request.url, let root = Bundle.main.url(forResource: "DocumentViews", withExtension: nil) else {
                continuation.finish(throwing: URLError(.resourceUnavailable))
                return
            }
            let file = root.appending(path: String(url.path().dropFirst())).standardizedFileURL
            guard file.path().hasPrefix(root.standardizedFileURL.path()), let content = try? Data(contentsOf: file),
                  let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: [
                      "Content-Type": Self.types[file.pathExtension] ?? "application/octet-stream",
                      "Content-Security-Policy": CanvasDocumentPage.policy,
                  ])
            else {
                continuation.finish(throwing: URLError(.fileDoesNotExist))
                return
            }
            continuation.yield(.response(response))
            continuation.yield(.data(content))
            continuation.finish()
        }
    }
}

private struct DocumentNavigation: WebPage.NavigationDeciding {
    let links: CanvasLinks

    func decidePolicy(for action: WebPage.NavigationAction, preferences _: inout WebPage.NavigationPreferences) async -> WKNavigationActionPolicy {
        guard let url = action.request.url else { return .cancel }
        if url.scheme == CanvasDocumentPage.scheme {
            return .allow
        }
        if action.navigationType == .linkActivated {
            links.follow(url)
        }
        return .cancel
    }
}
