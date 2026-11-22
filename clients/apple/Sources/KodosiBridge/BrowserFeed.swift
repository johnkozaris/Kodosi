import CoreGraphics
import CoreMedia
import Foundation
import ImageIO
import Observation
import Synchronization

/// Each frame is acknowledged, so the host keeps the picture as sharp as the link allows and no
/// queue forms. A frame comes on the task of the browser pipes, where it is acknowledged and
/// shown, so a busy main thread never looks like a slow link; what a view reads goes to the main
/// actor in the order of the pipe.
@MainActor
@Observable
public final class BrowserFeed {
    public enum Picture: Sendable {
        case waiting
        case live
        /// The surface keeps the last picture while the feed joins again, until a new frame
        /// replaces it.
        case kept
    }

    private enum Heard: Sendable {
        case news(DocumentLink.News)
        case state(BrowserWindow)
        /// The surface with this number shows a picture.
        case shown(Int)
        case sized(CGSize)
    }

    private struct Video {
        var link: DocumentLink?
        var teller: AsyncStream<Heard>.Continuation?
        var display: (@Sendable (CMSampleBuffer) -> Bool)?
        var surface = 0
        var shown = false
        var samples = H264Samples()
        var lastFrame: UInt32 = 0
        var holds = Holds.nothing
        var pictureSize: CGSize?
    }

    private nonisolated static let frameHeader = 6
    private nonisolated static let messageTag: UInt8 = 0
    private nonisolated static let frameTag: UInt8 = 1
    private nonisolated static let keyFlag: UInt8 = 1
    private nonisolated static let anchorFlag: UInt8 = 2
    private static let settle = Duration.milliseconds(250)
    #if os(macOS)
        private static let hostsLayer = NSClassFromString("CALayerHost") != nil
    #else
        private static let hostsLayer = false
    #endif

    public private(set) var window = BrowserWindow()
    public private(set) var icons: [String: CGImage] = [:]
    public private(set) var picture = Picture.waiting
    /// The size of the pictures that the video shows. While the page takes a new size, the host
    /// gives that size before the page has drawn in it, so for a moment the pictures still have
    /// the size before.
    public private(set) var pictureSize: CGSize?
    public private(set) var failure: String?
    /// Counts each press, key and scroll of a person on this device.
    public private(set) var touches = 0
    @ObservationIgnored public var copied: (String) -> Void = { _ in }
    /// The page takes the shape of the view of the person who acted last, so a new size goes to
    /// the host once the view settles, never while a button is held, and one that waits goes
    /// right after the person's act: the act then reaches the page that the person saw.
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
    private nonisolated let video = Mutex(Video())
    @ObservationIgnored private weak var surface: AnyObject?
    @ObservationIgnored private var surfaces = 0
    @ObservationIgnored private var sentSize: BrowserSize?
    @ObservationIgnored private var settling: Task<Void, Never>?
    @ObservationIgnored private var holding = false
    @ObservationIgnored private var joined = false
    /// A person opened the browser on this device, and the page did not take the shape of this
    /// view since: a view that only comes back on the screen, or joins again, leaves the page to
    /// the device in use.
    @ObservationIgnored private var opened = false

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    /// Only one view at a time watches, until it ends.
    public func watch() async {
        guard video.withLock({ $0.link == nil }) else { return }
        let link = store.browser(of: session)
        let (heard, teller) = AsyncStream.makeStream(of: Heard.self)
        video.withLock { video in
            video.link = link
            video.teller = teller
        }
        link.onFrame = { [weak self] in self?.receive($0) }
        link.onNews = { [weak self] in self?.hear($0) }
        defer {
            video.withLock { video in
                video.link = nil
                video.teller = nil
            }
            teller.finish()
            link.close()
            joined = false
            opened = false
        }
        do {
            try await link.open()
        } catch {
            // The view ends on each trip off the screen, which is no problem of the browser.
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

    /// A person opened the browser on this device, so the page takes the shape of this view: at
    /// once while it watches, or else when it joins.
    public func open() {
        opened = true
        if joined {
            join()
        }
    }

    /// A surface that is new to the feed has no picture, so the host sends a key frame at once.
    /// The display returns false when it lost the stream and waits for a key frame. The page's
    /// layer is no picture of the surface.
    public func show(on surface: AnyObject, _ display: @escaping @Sendable (CMSampleBuffer) -> Bool) {
        let fresh = surface !== self.surface
        if fresh {
            self.surface = surface
            surfaces += 1
            if window.layer == nil {
                picture = .waiting
            }
        }
        let number = surfaces
        video.withLock { video in
            video.display = display
            if video.surface != number {
                video.surface = number
                video.shown = false
            }
        }
        if fresh {
            restart()
        }
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

    public func send(_ event: BrowserInput) {
        post(.input(event))
        guard !event.isPlainMove else { return }
        touches += 1
        if case let .mouse(action, _, _, _, _, _, _, _) = event {
            if action == .mousePressed {
                holding = true
            } else if action == .mouseReleased {
                holding = false
            }
        }
        sendSize()
    }

    /// The files go into the folder of the terminal first, because the browser runs on the
    /// computer of the terminal.
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

    /// While a button is held, a point beyond the page goes to its nearest edge, so that the page
    /// also gets the rest of the drag and its release.
    public func browserPoint(_ point: CGPoint, in size: CGSize) -> BrowserInput.Point? {
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
                                         clamped: Bool = false) -> BrowserInput.Point?
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
        return BrowserInput.Point(x: Double(x), y: Double(y))
    }

    private func join() {
        failure = nil
        joined = true
        video.withLock { $0.holds = .nothing }
        let size = currentSize
        sentSize = size
        post(.watch(size, opened: opened, layer: Self.hostsLayer))
        opened = false
    }

    private var currentSize: BrowserSize {
        BrowserSize(width: Int(viewSize.width), height: Int(viewSize.height), scale: Double(viewScale))
    }

    private func settleSize() {
        settling?.cancel()
        settling = Task { [weak self] in
            try? await Task.sleep(for: Self.settle)
            guard !Task.isCancelled else { return }
            self?.sendSize()
        }
    }

    private func sendSize() {
        let size = currentSize
        guard !holding, size != sentSize else { return }
        sentSize = size
        post(.size(size))
    }

    private nonisolated func restart() {
        video.withLock { $0.holds = .nothing }
        post(.keyframe)
    }

    private nonisolated func post(_ message: ToBrowserWindow) {
        guard let body = try? JSONEncoder().encode(message) else { return }
        video.withLock { $0.link }?.send(Data([Self.messageTag]) + body)
    }

    nonisolated func receive(_ message: Data) {
        switch message.first {
        case Self.frameTag:
            frame(message)
        case Self.messageTag:
            if let state = try? JSONDecoder().decode(BrowserWindow.self, from: message.dropFirst()) {
                tell(.state(state))
            }
        default:
            break
        }
    }

    private nonisolated func hear(_ news: DocumentLink.News) {
        if case .lost = news {
            video.withLock { $0.shown = false }
        }
        tell(.news(news))
    }

    private nonisolated func tell(_ heard: Heard) {
        video.withLock { $0.teller }?.yield(heard)
    }

    private func take(_ heard: Heard) {
        switch heard {
        case let .news(.lost(reason)):
            joined = false
            if picture == .live {
                picture = .kept
            }
            failure = reason
        case let .state(state):
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
        case let .sized(size):
            pictureSize = size
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

    /// The host skips frames for a view that falls behind and goes on at an anchor, so only a
    /// delta frame after a gap shows a frame lost on the way, and the decoder needs a key frame.
    private nonisolated func frame(_ message: Data) {
        guard message.count > Self.frameHeader else { return }
        let header = [UInt8](message.prefix(Self.frameHeader))
        let id = header[1 ..< 5].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }
        let key = header[5] & Self.keyFlag != 0
        let anchor = header[5] & Self.anchorFlag != 0
        post(.ack(id))
        let (restarts, heard) = video.withLock { video -> (Bool, [Heard]) in
            if video.holds == .stream, id != video.lastFrame &+ 1 {
                video.holds = .gap
            }
            video.lastFrame = id
            switch video.holds {
            case .nothing where !key:
                return (false, [])
            case .gap where !key && !anchor:
                return (true, [])
            default:
                break
            }
            guard let sample = video.samples.sample(of: message.dropFirst(Self.frameHeader)) else { return (false, []) }
            video.holds = .stream
            let going = video.display?(sample) ?? true
            var heard: [Heard] = []
            if let size = sample.formatDescription?.presentationDimensions(), size != video.pictureSize {
                video.pictureSize = size
                heard.append(.sized(size))
            }
            if !video.shown {
                video.shown = true
                heard.append(.shown(video.surface))
            }
            return (!going, heard)
        }
        if restarts {
            restart()
        }
        heard.forEach(tell)
    }
}

/// What the decoder of a feed holds.
private enum Holds {
    /// Nothing that a frame after a key frame needs, as a new decoder.
    case nothing
    /// Each frame since its key frame.
    case stream
    /// Every frame up to one that the host skipped.
    case gap
}

private enum ToBrowserWindow: Encodable {
    case watch(BrowserSize, opened: Bool, layer: Bool)
    case size(BrowserSize)
    case ack(UInt32)
    case keyframe
    case input(BrowserInput)

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .watch(size, opened, layer):
            try container.encode("watch", forKey: .type)
            try container.encode(size, forKey: .size)
            try container.encode(true, forKey: .anchors)
            try container.encode(opened, forKey: .opened)
            try container.encode(layer, forKey: .layer)
        case let .size(size):
            try container.encode("size", forKey: .type)
            try container.encode(size, forKey: .size)
        case let .ack(frame):
            try container.encode("ack", forKey: .type)
            try container.encode(frame, forKey: .frame)
        case .keyframe:
            try container.encode("keyframe", forKey: .type)
        case let .input(input):
            try container.encode("input", forKey: .type)
            try container.encode(input, forKey: .input)
        }
    }

    private enum Field: String, CodingKey { case type, size, frame, input, anchors, opened, layer }
}

private struct BrowserSize: Encodable, Equatable {
    let width: Int
    let height: Int
    let scale: Double
}
