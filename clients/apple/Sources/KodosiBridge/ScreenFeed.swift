import CoreGraphics
import Foundation
import Observation
import os

/// The screen of a terminal's computer on its canvas: the video of the display or window that it
/// shows, its pointer, and the zoom of this view. Input and pointer positions are in the shown
/// source's points.
@MainActor
@Observable
public final class ScreenFeed {
    private static let settle = Duration.milliseconds(250)
    /// How often a zoom or a pan that goes on moves the capture to what the view shows.
    private static let lookEvery = Duration.milliseconds(200)
    /// The closest view, in view points per source point.
    private static let closest: CGFloat = 4
    /// The most clipboard text that the computer takes, in UTF-8 bytes.
    private static let mostClipboard = 1024 * 1024

    public private(set) var picture = VideoPicture.waiting
    /// What broke unexpectedly, which the view does not say but lets the person try again.
    public private(set) var failure: String?
    /// False while the person at the computer does not share its screen, when nothing watches it.
    public private(set) var shared = true
    public private(set) var offline = false
    public private(set) var away: ScreenAway?
    /// False while another device drives the capture, whose view this one shows.
    public private(set) var control = true
    /// The part of the source that the frames show while another device drives the capture.
    private var followed: CGRect?
    public var still: VideoStill?
    /// Text copied on the computer, for this device's clipboard.
    @ObservationIgnored public var copied: (String) -> Void = { _ in }
    public private(set) var sources = ScreenSources()
    public private(set) var source: ScreenSource?
    /// The shown source's size in points.
    public private(set) var size = CGSize.zero
    public private(set) var pointer: ScreenPointer?
    public private(set) var shapes: [UInt64: ScreenShape] = [:]
    /// What macOS lets Kodosi Screen do, once the screen said.
    public private(set) var access: ScreenAccess?
    /// How much closer than the whole source the view shows, from 1.
    public private(set) var zoom: CGFloat = 1
    /// The source point in the middle of the view while it is zoomed.
    private var center = CGPoint.zero
    /// The capture's size follows the view that the person opened or used last.
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
    private let channel = VideoChannel<ScreenMessage>()
    @ObservationIgnored private weak var surface: AnyObject?
    @ObservationIgnored private var surfaces = 0
    @ObservationIgnored private var sentSize: VideoSize?
    @ObservationIgnored private var settling: Task<Void, Never>?
    /// The part of the source that the screen captures for this view, or `nil` for all of it.
    @ObservationIgnored private var sentLook: CGRect?
    @ObservationIgnored private var lookSent: ContinuousClock.Instant?
    @ObservationIgnored private var looking: Task<Void, Never>?
    @ObservationIgnored private var holding = false
    /// Set when this view acts, until the screen says that this device drives the capture.
    @ObservationIgnored private var taking = false
    @ObservationIgnored private var joined = false
    /// Set when the person opens the screen here, so that its capture follows this view.
    @ObservationIgnored private var opened = false

    init(store: CanvasStore, session: String) {
        self.store = store
        self.session = session
    }

    public var showing: VideoShowing {
        if offline {
            return .away(.offline)
        }
        if !shared {
            return .away(.sharingOff)
        }
        switch away {
        case .locked: return .away(.locked)
        case .anotherAccount: return .away(.anotherAccount)
        case .asleep: return .away(.asleep)
        case nil: break
        }
        if access?.screen == false {
            return .away(.unrecorded)
        }
        if failure != nil {
            return .failed
        }
        if picture == .live {
            return .live
        }
        return picture == .kept || still != nil ? .resting : .opening
    }

    /// Watches while the screen is shared; a screen that stops being shared ends the watch.
    public func watch() async {
        guard shared else { return }
        let link = store.screen(of: session)
        guard let heard = channel.start(link) else { return }
        failure = nil
        defer {
            channel.stop()
            joined = false
            opened = false
            if picture == .live {
                picture = .kept
            }
        }
        do {
            try await link.open()
        } catch {
            // Leaving the screen cancels the view's task, which is not a failure of the screen.
            if !Task.isCancelled {
                failure = error.localizedDescription
            }
            return
        }
        for await item in heard {
            switch item {
            case .news(.open):
                // A new pipe is a new seat at the screen, which shows the whole source.
                sentLook = nil
                taking = false
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

    /// Attaches a video surface. A new surface asks for a keyframe at once.
    public func show(on surface: AnyObject, _ display: any VideoDisplay) {
        let fresh = surface !== self.surface
        if fresh {
            self.surface = surface
            surfaces += 1
            picture = .waiting
        }
        channel.show(on: surfaces, display)
        if fresh {
            channel.restart()
        }
    }

    /// Detaches a surface that goes away, so that its display goes with it. Returns whether the
    /// surface was the one that showed the feed.
    @discardableResult
    public func hide(from surface: AnyObject) -> Bool {
        guard surface === self.surface else { return false }
        self.surface = nil
        channel.hide()
        return true
    }

    public func send(_ input: ViewerInput) {
        channel.post(ToScreen.input(input))
        if case let .mouse(action, _, _, _, _, _, _, _) = input {
            if action == .mousePressed {
                holding = true
            } else if action == .mouseReleased {
                holding = false
            }
        }
        if input.acts {
            takeControl()
        }
        if !input.isPlainMove {
            sendSize()
        }
    }

    /// Gives the computer this device's clipboard, which it pastes next.
    public func sendClipboard(_ text: String) {
        guard !text.isEmpty, text.utf8.count <= Self.mostClipboard else { return }
        channel.post(ToScreen.clipboard(text))
    }

    /// Turns the sharing of this computer's own screen on, which has macOS ask for what Kodosi
    /// Screen may not do yet.
    public func share() async {
        do {
            try await store.shareScreen()
        } catch {
            Logger.app.error("The screen was not shared: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// What the canvas says about the screen and its computer.
    func see(_ session: RuntimeSession) {
        let shared = session.canvas?.screen?.shared ?? false
        if shared != self.shared {
            self.shared = shared
            if !shared {
                channel.stop()
            }
        }
        if session.hostOffline != offline {
            offline = session.hostOffline
        }
    }

    public func show(_ source: ScreenSource) {
        channel.post(ToScreen.show(source))
    }

    /// Where the source shows in a view of `view` size, or `nil` before the screen said its size.
    /// While another device drives the capture, the view shows what that device shows.
    public func placement(in view: CGSize) -> VideoPlacement? {
        guard size.width > 0, size.height > 0, view.width > 0, view.height > 0 else { return nil }
        let whole = CGRect(origin: .zero, size: size)
        guard control else { return VideoPlacement(source: size, look: followed ?? whole) }
        guard zoom > 1 else { return VideoPlacement(source: size, look: whole) }
        let fit = Self.fit(size, in: view) * zoom
        let width = min(view.width / fit, size.width)
        let height = min(view.height / fit, size.height)
        let look = CGRect(x: min(max(center.x - width / 2, 0), size.width - width),
                          y: min(max(center.y - height / 2, 0), size.height - height), width: width, height: height)
        return VideoPlacement(source: size, look: look)
    }

    /// The source point under a view point. While a button is held, points outside the source
    /// keep to its edge, so that a drag that leaves the view still reaches the computer.
    public func point(_ point: CGPoint, in view: CGSize) -> ViewerInput.Point? {
        placement(in: view)?.sourcePoint(point, in: view, clamped: holding).map { ViewerInput.Point(x: $0.x, y: $0.y) }
    }

    /// Zooms by `factor`, keeping the source point under `anchor` where it is.
    public func zoom(by factor: CGFloat, at anchor: CGPoint, in view: CGSize) {
        guard let before = placement(in: view), let held = before.sourcePoint(anchor, in: view, clamped: true) else { return }
        let most = max(1, Self.closest / Self.fit(size, in: view))
        let zoom = min(max(zoom * factor, 1), most)
        guard zoom != self.zoom else { return }
        takeControl()
        self.zoom = zoom
        let fit = Self.fit(size, in: view) * zoom
        center = CGPoint(x: held.x - (anchor.x - view.width / 2) / fit, y: held.y - (anchor.y - view.height / 2) / fit)
        looked(in: view)
    }

    /// Moves a zoomed view by `distance` in view points.
    public func pan(by distance: CGSize, in view: CGSize) {
        guard zoom > 1 else { return }
        takeControl()
        let fit = Self.fit(size, in: view) * zoom
        center = CGPoint(x: center.x + distance.width / fit, y: center.y + distance.height / fit)
        looked(in: view)
    }

    public func showWhole() {
        guard zoom > 1 else { return }
        takeControl()
        zoom = 1
        sendLook()
    }

    /// Acting here takes the capture over, so this view shows its own look at once.
    private func takeControl() {
        taking = true
        if !control {
            control = true
        }
    }

    private static func fit(_ source: CGSize, in view: CGSize) -> CGFloat {
        min(view.width / source.width, view.height / source.height)
    }

    /// Keeps the middle where the source is, so that a pan back starts at once.
    private func looked(in view: CGSize) {
        if let look = placement(in: view)?.look {
            center = CGPoint(x: look.midX, y: look.midY)
        }
        sendLook()
    }

    private func join() {
        failure = nil
        joined = true
        let size = currentSize
        sentSize = size
        channel.post(ToScreen.watch(size, decoders: DeviceDecoder.here, opened: opened, commandKey: true))
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
            self?.sendLook()
        }
    }

    func sendSize() {
        let size = currentSize
        guard joined, size != sentSize else { return }
        sentSize = size
        channel.post(ToScreen.size(size))
    }

    /// Sends what the view shows at most every `lookEvery`, and the last of a zoom or pan once
    /// it ends. Each look names its source, so the screen leaves one that comes after another
    /// source took its place. A look takes the capture over, so a device sends one only while it
    /// drives the capture or acts.
    private func sendLook() {
        guard joined, control, let source else { return }
        let look = zoom > 1 ? placement(in: viewSize)?.look : nil
        guard look != sentLook else { return }
        looking?.cancel()
        let now = ContinuousClock.now
        if let lookSent, now - lookSent < Self.lookEvery {
            looking = Task { [weak self] in
                try? await Task.sleep(until: lookSent + Self.lookEvery)
                guard !Task.isCancelled else { return }
                self?.sendLook()
            }
            return
        }
        sentLook = look
        lookSent = now
        channel.post(ToScreen.look(look, of: source))
    }

    private func take(_ heard: VideoChannel<ScreenMessage>.Heard) {
        switch heard {
        case let .news(.lost(reason)):
            joined = false
            if picture == .live {
                picture = .kept
            }
            failure = reason
        case let .message(message):
            take(message)
        case let .shown(number):
            if number == surfaces {
                picture = .live
                still = nil
            }
        case let .region(region):
            followed = region
        case .repairing:
            if picture == .live {
                picture = .kept
            }
        case .news(.open), .news(.closed):
            break
        }
    }

    private func take(_ message: ScreenMessage) {
        if let sources = message.sources, sources != self.sources {
            self.sources = sources
        }
        // The screen shows another source whole to every view.
        if message.source != source {
            source = message.source
            zoom = 1
            sentLook = nil
        }
        let size = CGSize(width: message.width, height: message.height)
        if size != self.size {
            self.size = size
        }
        if message.pointer != pointer {
            pointer = message.pointer
        }
        if message.access != access {
            access = message.access
        }
        if message.away != away {
            away = message.away
            // The picture rests from now on, until the screen comes back with new frames.
            if away != nil, picture == .live {
                picture = .kept
                channel.rest()
            }
        }
        if message.control {
            taking = false
        }
        let control = message.control || taking
        if control != self.control {
            self.control = control
        }
        if control {
            sendLook()
        }
        for shape in message.shapes {
            shapes[shape.id] = shape.picture
        }
        if let text = message.copied {
            copied(text)
        }
    }
}

private enum ToScreen: Encodable {
    /// `commandKey` says that this device's keyboard has ⌘, which a computer that is not a Mac
    /// takes as Control in shortcuts.
    case watch(VideoSize, decoders: [DeviceDecoder], opened: Bool, commandKey: Bool)
    case size(VideoSize)
    case input(ViewerInput)
    case show(ScreenSource)
    case look(CGRect?, of: ScreenSource)
    case clipboard(String)

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Field.self)
        switch self {
        case let .watch(size, decoders, opened, commandKey):
            try container.encode("watch", forKey: .type)
            try container.encode(size, forKey: .size)
            try container.encode(decoders, forKey: .decoders)
            try container.encode(true, forKey: .anchors)
            try container.encode(opened, forKey: .opened)
            try container.encode(commandKey, forKey: .commandKey)
        case let .size(size):
            try container.encode("size", forKey: .type)
            try container.encode(size, forKey: .size)
        case let .input(input):
            try container.encode("input", forKey: .type)
            try container.encode(input, forKey: .input)
        case let .show(source):
            try container.encode("show", forKey: .type)
            try container.encode(source, forKey: .source)
        case let .look(look, source):
            try container.encode("look", forKey: .type)
            try container.encode(source, forKey: .source)
            try container.encode(look.map(Region.init), forKey: .region)
        case let .clipboard(text):
            try container.encode("clipboard", forKey: .type)
            try container.encode(text, forKey: .text)
        }
    }

    private enum Field: String, CodingKey { case type, size, decoders, anchors, opened, commandKey, input, source, region, text }
}

private struct Region: Encodable {
    let x: Double
    let y: Double
    let width: Double
    let height: Double

    init(_ rect: CGRect) {
        x = rect.minX
        y = rect.minY
        width = rect.width
        height = rect.height
    }
}
