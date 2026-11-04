import KodosiBridge
import SwiftUI
import WebKit

/// A live document, sheet, presentation or PDF. The view bundle draws it in a closed frame, and
/// its only way out is the pipe of the document.
struct CanvasDocumentView: View {
    @Environment(\.theme) private var theme
    @Environment(\.self) private var environment
    @Environment(\.openURL) private var openURL
    @AppStorage(DocumentViewers.storageKey) private var viewers = DocumentViewers()
    let session: String
    let item: CanvasItem
    let view: CanvasItem.View
    let store: CanvasStore
    let pages: CanvasDocumentPages
    let openBrowser: ((URL) -> Void)?
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
            page = pages.enter(item.key, in: session) {
                CanvasDocumentPage(link: store.document(item, in: session), view: view, look: look,
                                   preferences: viewers.preferences(for: view))
            }
            page?.links.open = { url in openURL(url) }
            page?.links.openBrowser = openBrowser
            page?.restyle(look)
            page?.prefer(viewers.preferences(for: view))
        }
        .onDisappear { pages.leave(item.key, in: session) }
        .onChange(of: look) { _, look in page?.restyle(look) }
        .onChange(of: viewers.preferences(for: view)) { _, preferences in page?.prefer(preferences) }
    }

    /// The look of the app, by the view's own names.
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

/// The pages of live documents on this device. A page that a person leaves stays open a while,
/// with its place and its undo. After that it closes, and the file has one view fewer.
@MainActor
public final class CanvasDocumentPages {
    private static let kept = Duration.seconds(300)

    private var pages: [String: CanvasDocumentPage] = [:]
    private var closing: [String: Task<Void, Never>] = [:]

    public init() {}

    fileprivate func enter(_ key: String, in session: String, open: () -> CanvasDocumentPage) -> CanvasDocumentPage {
        let place = "\(session)/\(key)"
        closing.removeValue(forKey: place)?.cancel()
        if let page = pages[place] {
            return page
        }
        let page = open()
        pages[place] = page
        return page
    }

    /// The words that a person selected in the page of a file, while the page is open.
    public func selectedWords(of key: String, in session: String) async -> String? {
        await pages["\(session)/\(key)"]?.selectedWords()
    }

    fileprivate func leave(_ key: String, in session: String) {
        let place = "\(session)/\(key)"
        closing[place] = Task { [weak self] in
            guard await (try? Task.sleep(for: Self.kept)) != nil else { return }
            self?.closing[place] = nil
            self?.pages.removeValue(forKey: place)?.close()
        }
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
    @ObservationIgnored private let view: CanvasItem.View
    @ObservationIgnored private var look: [String: String]
    @ObservationIgnored private var preferences: [String: Bool]
    @ObservationIgnored private var frames: AsyncStream<Data>.Continuation?
    @ObservationIgnored private var tasks: [Task<Void, Never>] = []
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
        configuration.userContentController.add(messages, contentWorld: .page, name: "document")
        configuration.userContentController.add(messages, contentWorld: .page, name: "ready")
        page = WebPage(configuration: configuration, navigationDecider: DocumentNavigation(links: links))
        let (stream, frames) = AsyncStream.makeStream(of: Data.self)
        self.frames = frames
        link.onFrame = { frames.yield($0) }
        link.onLost = { [weak self] in self?.tasks.append(Task { await self?.lost() }) }
        messages.frame = { link.send($0) }
        messages.ready = { [weak self] in self?.shown = true }
        tasks = [
            Task { [weak self] in
                for await frame in stream {
                    await self?.deliver(frame)
                }
            },
            Task { [weak self] in await self?.stayOpen() },
        ]
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
        frames?.finish()
        frames = nil
        link.close()
    }

    /// The system can end a web process, mostly on an iPhone. The view then starts again.
    private func stayOpen() async {
        let start = URL(string: "\(Self.scheme)://view/index.html")!
        while !Task.isCancelled {
            do {
                for try await _ in page.load(URLRequest(url: start)) {}
                _ = try await page.callJavaScript(
                    """
                    window.documentHost = window.documentView.hosted(
                      { kind, theme, preferences, ready: () => webkit.messageHandlers.ready.postMessage(true) },
                      (frame) => webkit.messageHandlers.document.postMessage(frame),
                    );
                    """,
                    arguments: ["kind": view.rawValue, "theme": look, "preferences": preferences],
                    contentWorld: .page,
                )
                await join()
                for try await _ in page.navigations {}
            } catch {
                shown = false
                link.close()
            }
            try? await Task.sleep(for: .seconds(1))
        }
    }

    private func lost() async {
        _ = try? await page.callJavaScript("documentHost.detach()", contentWorld: .page)
        await join()
    }

    /// Opens a pipe and joins the file on it. The computer of the terminal can be away for a
    /// time, so this goes on for as long as the page is open.
    private func join() async {
        var wait = Duration.seconds(1)
        while frames != nil {
            do {
                try await link.open()
                _ = try await page.callJavaScript("documentHost.attach()", contentWorld: .page)
                return
            } catch {
                wait = await link.pause(wait) ? .seconds(1) : min(wait * 2, .seconds(30))
            }
        }
    }

    private func deliver(_ frame: Data) async {
        let data = frame.base64EncodedString()
        _ = try? await page.callJavaScript("documentHost.deliver(data)", arguments: ["data": data], contentWorld: .page)
    }
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

/// Serves the view bundle of the app, with no way to the network.
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
