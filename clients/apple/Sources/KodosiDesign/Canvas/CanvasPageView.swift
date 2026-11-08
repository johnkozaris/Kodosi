#if os(macOS)
    import AppKit
#else
    import UIKit
#endif
import KodosiBridge
import KodosiLinks
import SwiftUI
import Synchronization
import WebKit

/// "Ask about this": the words that a person selected go to the agent of the terminal.
public struct CanvasAsk {
    public let title: String
    public let agent: AgentKind
    public let send: (String?) -> Void

    public init(title: String, agent: AgentKind, send: @escaping (String?) -> Void) {
        self.title = title
        self.agent = agent
        self.send = send
    }
}

/// A page of a canvas in a closed frame. It runs its own scripts, but it reaches no network,
/// no files and no storage that stays. A link opens on this device by the rules of terminal links.
/// Words that a person selects get a key that asks the agent about them. Each page view has a
/// frame of its own, so no web page ever shows in two places at once.
public struct CanvasPageView: View {
    @Environment(\.openURL) private var openURL
    @Environment(\.theme) private var theme
    let key: String
    let frames: CanvasFrames
    let content: Data
    let otherComputer: String?
    let ask: CanvasAsk?
    @State private var frame: CanvasFrame?
    @State private var shown = false

    public init(key: String, frames: CanvasFrames, content: Data, otherComputer: String?, ask: CanvasAsk? = nil) {
        self.key = key
        self.frames = frames
        self.content = content
        self.otherComputer = otherComputer
        self.ask = ask
    }

    public var body: some View {
        ZStack {
            if let frame {
                WebView(frame.page)
                    .webViewLinkPreviews(.disabled)
                    .webViewBackForwardNavigationGestures(.disabled)
                    .opacity(shown ? 1 : 0)
                    .overlay { selectionKey(frame) }
                    .task(id: content) {
                        frame.links.open = { url in openURL(url) }
                        frame.links.otherComputer = otherComputer
                        await frame.show(content)
                        withAnimation(theme.motion.fade) { shown = true }
                    }
            }
        }
        .onAppear {
            let frame = frame ?? CanvasFrame()
            self.frame = frame
            frames.hold(frame, for: key)
        }
        .onDisappear {
            if let frame {
                frames.release(frame, for: key)
            }
        }
    }

    private func selectionKey(_ frame: CanvasFrame) -> some View {
        GeometryReader { geometry in
            if let ask, let selection = frame.selection {
                AskKey(ask: ask) {
                    ask.send(selection.words)
                    frame.clearSelection()
                }
                .fixedSize()
                .position(x: min(max(selection.rect.midX, 130), max(130, geometry.size.width - 130)), y: Self.place(of: selection.rect))
                .transition(.scale(scale: 0.92, anchor: .bottom).combined(with: .opacity))
            }
        }
        .animation(theme.motion.snappy, value: frame.selection)
    }

    /// Above the words with a pointer; under them on a touch screen, where the system's menu is
    /// above.
    private static func place(of words: CGRect) -> CGFloat {
        #if os(iOS)
            words.maxY + 26
        #else
            max(18, words.minY - 20)
        #endif
    }
}

private struct AskKey: View {
    @Environment(\.theme) private var theme
    let ask: CanvasAsk
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                AgentMark(kind: ask.agent, size: 16)
                Text(ask.title).appTextStyle(.footnote).fontWeight(.semibold).foregroundStyle(theme.colors.accentStrong)
            }
            .padding(.leading, 6).padding(.trailing, 12).frame(height: 30)
            .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
            .contentShape(Capsule())
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityIdentifier("canvas.page.ask")
    }
}

/// The words that a person selected on a page and where they are, in the page's points.
public struct CanvasSelection: Equatable, Sendable {
    public let words: String
    public let rect: CGRect
}

/// The frames of the pages that a canvas shows on this device, by item, while their views show.
@MainActor
public final class CanvasFrames {
    private var frames: [String: CanvasFrame] = [:]

    public init() {}

    fileprivate func hold(_ frame: CanvasFrame, for key: String) {
        frames[key] = frame
    }

    fileprivate func release(_ frame: CanvasFrame, for key: String) {
        if frames[key] === frame {
            frames.removeValue(forKey: key)
        }
    }

    /// The words that the person selected on the page of an item, while it shows.
    public func selectedWords(of key: String) async -> String? {
        await frames[key]?.selectedWords()
    }
}

@MainActor
@Observable
final class CanvasFrame {
    nonisolated static let scheme = "kodosi-canvas"
    nonisolated static let policy = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; "
        + "font-src data:; media-src data: blob:; form-action 'none'; base-uri 'none'"
    /// Kodosi's own scripts run in a world of their own, which the page's scripts cannot reach.
    nonisolated static let world = "kodosi"
    nonisolated static let watch = """
    (() => {
      let held = false, asked = false;
      const tell = () => {
        asked = false;
        const selection = getSelection();
        const words = !held && selection && !selection.isCollapsed ? selection.toString().trim().slice(0, 2000) : "";
        let rect = null;
        if (words && selection.rangeCount) {
          const box = selection.getRangeAt(0).getBoundingClientRect();
          rect = { x: box.left, y: box.top, width: box.width, height: box.height };
        }
        webkit.messageHandlers.selection.postMessage({ words, rect });
      };
      const soon = () => { if (!asked) { asked = true; requestAnimationFrame(tell); } };
      document.addEventListener("selectionchange", soon);
      addEventListener("mousedown", () => { held = true; soon(); }, true);
      addEventListener("mouseup", () => { held = false; soon(); }, true);
      addEventListener("scroll", soon, { capture: true, passive: true });
      addEventListener("resize", soon);
    })();
    """

    @ObservationIgnored let page: WebPage
    @ObservationIgnored let links = CanvasLinks()
    private(set) var selection: CanvasSelection?
    @ObservationIgnored private let document = CanvasDocument()
    @ObservationIgnored private let watcher = SelectionWatcher()
    @ObservationIgnored private var shown = 0

    init() {
        var configuration = WebPage.Configuration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.urlSchemeHandlers[URLScheme(Self.scheme)!] = CanvasSchemeHandler(document: document)
        configuration.upgradeKnownHostsToHTTPS = false
        let world = WKContentWorld.world(name: Self.world)
        configuration.userContentController.addUserScript(WKUserScript(source: Self.watch, injectionTime: .atDocumentEnd,
                                                                       forMainFrameOnly: true, in: world))
        configuration.userContentController.add(watcher, contentWorld: world, name: "selection")
        page = WebPage(configuration: configuration, navigationDecider: CanvasNavigation(links: links))
        watcher.seen = { [weak self] selection in
            if self?.selection != selection {
                self?.selection = selection
            }
        }
    }

    func show(_ content: Data) async {
        let place = shown > 0 ? try? await page.callJavaScript("return [scrollX, scrollY]", contentWorld: .defaultClient) as? [Double] : nil
        shown += 1
        selection = nil
        let url = document.publish(content, as: shown)
        do {
            for try await _ in page.load(URLRequest(url: url)) {}
        } catch { return }
        if let place, place.count == 2 {
            _ = try? await page.callJavaScript("scrollTo(x, y)", arguments: ["x": place[0], "y": place[1]], contentWorld: .defaultClient)
        }
    }

    /// The words that the person selected on the page.
    func selectedWords() async -> String? {
        try? await page.callJavaScript("return getSelection().toString()", contentWorld: .defaultClient) as? String
    }

    func clearSelection() {
        selection = nil
        Task { _ = try? await page.callJavaScript("getSelection().removeAllRanges()", contentWorld: .defaultClient) }
    }
}

@MainActor
private final class SelectionWatcher: NSObject, WKScriptMessageHandler {
    var seen: (CanvasSelection?) -> Void = { _ in }

    func userContentController(_: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let body = message.body as? [String: Any] else { return }
        let words = body["words"] as? String ?? ""
        guard !words.isEmpty, let rect = body["rect"] as? [String: Any],
              let x = (rect["x"] as? NSNumber)?.doubleValue, let y = (rect["y"] as? NSNumber)?.doubleValue,
              let width = (rect["width"] as? NSNumber)?.doubleValue, let height = (rect["height"] as? NSNumber)?.doubleValue
        else {
            seen(nil)
            return
        }
        seen(CanvasSelection(words: words, rect: CGRect(x: x, y: y, width: width, height: height)))
    }
}

/// The links that a person clicks on a page or in a file of a canvas.
@MainActor
final class CanvasLinks {
    var open: (URL) -> Void = { _ in }
    /// The computer of the terminal, when it is not this device.
    var otherComputer: String?
    var review: (ClickedLink) -> Void = { link in
        #if os(macOS)
            link.review(in: NSApp.keyWindow, from: .canvas)
        #else
            let window = UIApplication.shared.connectedScenes.lazy.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
            link.review(from: window?.rootViewController, origin: .canvas)
        #endif
    }

    func follow(_ url: URL) {
        let link = ClickedLink(url.absoluteString, otherComputer: otherComputer)
        switch link.decision {
        case let .open(target):
            open(target)
        case .ask, .refuse:
            review(link)
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
