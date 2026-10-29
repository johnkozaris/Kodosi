import KodosiBridge
import SwiftUI
import Synchronization
import WebKit

/// A page of a canvas in a closed frame. It runs its own scripts, but it reaches no network,
/// no files and no storage that stays. A web or mail link opens on this device.
public struct CanvasPageView: View {
    @Environment(\.openURL) private var openURL
    let frame: CanvasFrame
    let content: Data
    let otherComputer: String?

    public init(frame: CanvasFrame, content: Data, otherComputer: String?) {
        self.frame = frame
        self.content = content
        self.otherComputer = otherComputer
    }

    public var body: some View {
        WebView(frame.page)
            .webViewLinkPreviews(.disabled)
            .webViewBackForwardNavigationGestures(.disabled)
            .task(id: content) {
                frame.links.open = { url in openURL(url) }
                frame.links.otherComputer = otherComputer
                await frame.show(content)
            }
    }
}

@MainActor
public final class CanvasFrame {
    nonisolated static let scheme = "kodosi-canvas"
    nonisolated static let policy = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; "
        + "font-src data:; media-src data: blob:; form-action 'none'; base-uri 'none'"

    let page: WebPage
    let links = CanvasLinks()
    private let document = CanvasDocument()
    private var shown = 0

    public init() {
        var configuration = WebPage.Configuration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.urlSchemeHandlers[URLScheme(Self.scheme)!] = CanvasSchemeHandler(document: document)
        configuration.upgradeKnownHostsToHTTPS = false
        page = WebPage(configuration: configuration, navigationDecider: CanvasNavigation(links: links))
    }

    func show(_ content: Data) async {
        let place = shown > 0 ? try? await page.callJavaScript("return [scrollX, scrollY]", contentWorld: .defaultClient) as? [Double] : nil
        shown += 1
        let url = document.publish(content, as: shown)
        do {
            for try await _ in page.load(URLRequest(url: url)) {}
        } catch { return }
        if let place, place.count == 2 {
            _ = try? await page.callJavaScript("scrollTo(x, y)", arguments: ["x": place[0], "y": place[1]], contentWorld: .defaultClient)
        }
    }

    /// The words that the person selected on the page.
    public func selection() async -> String? {
        try? await page.callJavaScript("return getSelection().toString()", contentWorld: .defaultClient) as? String
    }
}

@MainActor
final class CanvasLinks {
    var open: (URL) -> Void = { _ in }
    var otherComputer: String?

    /// Web and mail links open on this device. A local address of another computer is on that
    /// computer, so it does not open.
    func follow(_ url: URL) {
        switch url.scheme?.lowercased() {
        case "http", "https":
            guard let host = url.host(), !host.isEmpty else { return }
            if otherComputer != nil, ["localhost", "127.0.0.1", "::1", "[::1]"].contains(host.lowercased()) || host.hasSuffix(".localhost") {
                return
            }
            open(url)
        case "mailto":
            open(url)
        default:
            return
        }
    }
}

final class CanvasDocument: Sendable {
    private let current = Mutex<(number: Int, content: Data)>((0, Data()))

    func publish(_ content: Data, as number: Int) -> URL {
        current.withLock { $0 = (number, content) }
        return URL(string: "\(CanvasFrame.scheme)://page/\(number)")!
    }

    func content(for url: URL) -> Data? {
        current.withLock { url.host() == "page" && url.path() == "/\($0.number)" ? $0.content : nil }
    }
}

struct CanvasSchemeHandler: URLSchemeHandler {
    let document: CanvasDocument

    func reply(for request: URLRequest) -> AsyncThrowingStream<URLSchemeTaskResult, any Error> {
        AsyncThrowingStream { continuation in
            guard let url = request.url, let content = document.content(for: url),
                  let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: [
                      "Content-Type": "text/html; charset=utf-8",
                      "Content-Security-Policy": CanvasFrame.policy,
                      "Cache-Control": "no-store",
                  ])
            else {
                continuation.finish(throwing: URLError(.resourceUnavailable))
                return
            }
            continuation.yield(.response(response))
            continuation.yield(.data(content))
            continuation.finish()
        }
    }
}

struct CanvasNavigation: WebPage.NavigationDeciding {
    let links: CanvasLinks

    func decidePolicy(for action: WebPage.NavigationAction, preferences _: inout WebPage.NavigationPreferences) async -> WKNavigationActionPolicy {
        guard let url = action.request.url else { return .cancel }
        if url.scheme == CanvasFrame.scheme || url.absoluteString == "about:blank" || url.absoluteString == "about:srcdoc" {
            return action.shouldPerformDownload ? .cancel : .allow
        }
        if action.navigationType == .linkActivated {
            links.follow(url)
        }
        return .cancel
    }
}
