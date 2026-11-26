import CoreGraphics
import Foundation
import ImageIO
import Observation

/// The shared browser of a terminal's canvas: its video on the `VideoChannel`, and its tabs,
/// pointers, menus and dialogs in pipe order on the main actor.
@MainActor
@Observable
public final class BrowserFeed {
    private static let settle = Duration.milliseconds(250)
    #if os(macOS)
        private static let hostsLayer = NSClassFromString("CALayerHost") != nil
    #else
        private static let hostsLayer = false
    #endif

    public private(set) var window = BrowserWindow()
    public private(set) var icons: [String: CGImage] = [:]
    public private(set) var picture = VideoPicture.waiting
    public private(set) var failure: String?
    /// Incremented for each local user input event (excluding plain pointer moves).
    public private(set) var touches = 0
    @ObservationIgnored public var copied: (String) -> Void = { _ in }
    /// The host resizes the page to match whichever viewer interacted last. Size updates are
    /// debounced, deferred while a mouse button is held, and flushed after local input so the
    /// interaction lands on the layout the user saw.
    @ObservationIgnored public var viewSize = CGSize.zero {
        didSet {
            if viewSize != oldValue {
                settleSize()
            }
        }
    }

    @ObservationIgnored public var viewScale: CGFloat = 1 {
        didSet {
            if viewScale != oldValue {
                settleSize()
            }
        }
    }

    @ObservationIgnored private let store: CanvasStore
    @ObservationIgnored private let session: String
    private let channel = VideoChannel<BrowserWindow>()
    @ObservationIgnored private weak var surface: AnyObject?
    @ObservationIgnored private var surfaces = 0
    @ObservationIgnored private var sentSize: VideoSize?
    @ObservationIgnored private var settling: Task<Void, Never>?
    @ObservationIgnored private var holding = false
    @ObservationIgnored private var joined = false
    /// Set when the user explicitly opens the browser here; cleared once sent in `watch`.
    /// Returning to screen or reconnecting leaves `opened` false so another active device keeps
    /// control of the page size.
    @ObservationIgnored private var opened = false

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    public func watch() async {
        let link = store.browser(of: session)
        guard let heard = channel.start(link) else { return }
        defer {
            channel.stop()
            joined = false
            opened = false
        }
        do {
            try await link.open()
        } catch {
            // Leaving the screen cancels the view's task, which is not a browser failure.
            if !Task.isCancelled {
                failure = error.localizedDescription
            }
            return
        }
        for await item in heard {
            switch item {
            case .news(.open):
                join()
            case .news(.closed):
                return
            default:
                take(item)
            }
        }
    }

    public func open() {
        opened = true
        if joined {
            join()
        }
    }

    /// Attaches a video surface. A new surface triggers an immediate keyframe request (leaving
    /// `picture` intact when `window.layer` renders locally).
    public func show(on surface: AnyObject, _ display: any VideoDisplay) {
        let fresh = surface !== self.surface
        if fresh {
            self.surface = surface
            surfaces += 1
            if window.layer == nil {
                picture = .waiting
            }
        }
        channel.show(on: surfaces, display)
        if fresh {
            channel.restart()
        }
    }

    /// Detaches a surface that goes away, so that its display goes with it.
    public func hide(from surface: AnyObject) {
        guard surface === self.surface else { return }
        self.surface = nil
        channel.hide()
    }

    public func wake() async {
        open()
        await run(.wake)
    }

    public func run(_ act: BrowserAct) async {
        do {
            try await store.act(act, in: session)
            failure = nil
        } catch {
            failure = error.localizedDescription
        }
    }

    public func send(_ event: ViewerInput) {
        send(.viewer(event))
    }

    public func send(_ event: BrowserInput) {
        channel.post(ToBrowserWindow.input(event))
        if case let .viewer(input) = event, input.isPlainMove {
            return
        }
        touches += 1
        if case let .viewer(.mouse(action, _, _, _, _, _, _, _)) = event {
            if action == .mousePressed {
                holding = true
            } else if action == .mouseReleased {
                holding = false
            }
        }
        sendSize()
    }

    /// Uploads selected files to the terminal's host first because the browser runs there.
    public func give(_ files: [URL]) async {
        var paths: [String] = []
        do {
            for file in files {
                if let path = try await store.upload(file, to: session, page: true) {
                    paths.append(path)
                }
            }
        } catch {
            failure = error.localizedDescription
        }
        send(.files(paths))
    }

    /// Clamps out-of-bounds coordinates to the page edge while a button is held so drags and
    /// releases outside the viewport still reach the page.
    public func browserPoint(_ point: CGPoint, in size: CGSize) -> ViewerInput.Point? {
        Self.point(point, in: size, browser: window.size, clamped: holding)
    }

    public func viewPoint(x: Double, y: Double, in size: CGSize) -> CGPoint? {
        Self.viewPoint(CGPoint(x: x, y: y), in: size, browser: window.size)
    }

    public nonisolated static func viewPoint(_ point: CGPoint, in size: CGSize, browser: CGSize) -> CGPoint? {
        guard browser.width > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / browser.width, size.height / browser.height)
        let origin = CGPoint(x: (size.width - browser.width * fit) / 2, y: (size.height - browser.height * fit) / 2)
        return CGPoint(x: origin.x + point.x * fit, y: origin.y + point.y * fit)
    }

    public nonisolated static func point(_ point: CGPoint, in size: CGSize, browser: CGSize,
                                         clamped: Bool = false) -> ViewerInput.Point?
    {
        guard browser.width > 0, browser.height > 0, size.width > 0, size.height > 0 else { return nil }
        let fit = min(size.width / browser.width, size.height / browser.height)
        let origin = CGPoint(x: (size.width - browser.width * fit) / 2, y: (size.height - browser.height * fit) / 2)
        var x = (point.x - origin.x) / fit
        var y = (point.y - origin.y) / fit
        if clamped {
            x = min(max(x, 0), browser.width.nextDown)
            y = min(max(y, 0), browser.height.nextDown)
        }
        guard x >= 0, y >= 0, x < browser.width, y < browser.height else { return nil }
        return ViewerInput.Point(x: Double(x), y: Double(y))
    }

    private func join() {
        failure = nil
        joined = true
        channel.rejoin()
        let size = currentSize
        sentSize = size
        channel.post(ToBrowserWindow.watch(size, decoders: DeviceDecoder.here, opened: opened, layer: Self.hostsLayer))
        opened = false
    }

    private var currentSize: VideoSize {
        VideoSize(viewSize, scale: viewScale, hot: store.hot)
    }

    private func settleSize() {
        settling?.cancel()
        settling = Task { [weak self] in
            try? await Task.sleep(for: Self.settle)
            guard !Task.isCancelled else { return }
            self?.sendSize()
        }
    }

    func sendSize() {
        let size = currentSize
        guard !holding, size != sentSize else { return }
        sentSize = size
        channel.post(ToBrowserWindow.size(size))
    }

    private func take(_ heard: VideoChannel<BrowserWindow>.Heard) {
        switch heard {
        case let .news(.lost(reason)):
            joined = false
            if picture == .live {
                picture = .kept
            }
            failure = reason
        case let .message(state):
            window = state
            if state.layer != nil {
                picture = .live
            }
            keep(state.icons, of: state.tabs)
            if let words = state.copied {
                copied(words)
            }
        case let .shown(number):
            if number == surfaces {
                picture = .live
            }
        case .news(.open), .news(.closed):
            break
        }
    }

    private func keep(_ changed: [String: String], of tabs: [BrowserWindow.Tab]) {
        var kept = icons
        for (tab, png) in changed {
            kept[tab] = Data(base64Encoded: png).flatMap { data in
                CGImageSourceCreateWithData(data as CFData, nil).flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }
            }
        }
        let open = Set(tabs.map(\.id))
        kept = kept.filter { open.contains($0.key) }
        if kept.keys != icons.keys || !changed.isEmpty {
            icons = kept
        }
    }
}

private enum ToBrowserWindow: Encodable {
    case watch(VideoSize, decoders: [DeviceDecoder], opened: Bool, layer: Bool)
    case size(VideoSize)
    case input(BrowserInput)

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .watch(size, decoders, opened, layer):
            try container.encode("watch", forKey: .type)
            try container.encode(size, forKey: .size)
            try container.encode(decoders, forKey: .decoders)
            try container.encode(true, forKey: .anchors)
            try container.encode(opened, forKey: .opened)
            try container.encode(layer, forKey: .layer)
        case let .size(size):
            try container.encode("size", forKey: .type)
            try container.encode(size, forKey: .size)
        case let .input(input):
            try container.encode("input", forKey: .type)
            try container.encode(input, forKey: .input)
        }
    }

    private enum Field: String, CodingKey { case type, size, input, decoders, anchors, opened, layer }
}
