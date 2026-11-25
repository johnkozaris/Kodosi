import KodosiBridge
import QuartzCore
import SwiftUI

#if os(macOS)
    struct BrowserVideoView: NSViewRepresentable {
        let feed: BrowserFeed

        func makeNSView(context _: Context) -> VideoSurface {
            VideoSurface(feed: feed)
        }

        func updateNSView(_ view: VideoSurface, context _: Context) {
            view.show(feed)
        }
    }

    final class VideoSurface: NSView {
        private let player = VideoPlayer(layer: CAMetalLayer())

        init(feed: BrowserFeed) {
            super.init(frame: .zero)
            layer = player.layer
            wantsLayer = true
            show(feed)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override func layout() {
            super.layout()
            fit()
        }

        override func viewDidChangeBackingProperties() {
            super.viewDidChangeBackingProperties()
            fit()
        }

        func show(_ feed: BrowserFeed) {
            let player = player
            feed.show(on: self) { player.enqueue($0) }
        }

        private func fit() {
            player.resize(to: bounds.size, scale: window?.backingScaleFactor ?? 1)
        }
    }

#else
    struct BrowserVideoView: UIViewRepresentable {
        let feed: BrowserFeed

        func makeUIView(context _: Context) -> VideoSurface {
            VideoSurface(feed: feed)
        }

        func updateUIView(_ view: VideoSurface, context _: Context) {
            view.show(feed)
        }
    }

    final class VideoSurface: UIView {
        override static var layerClass: AnyClass {
            CAMetalLayer.self
        }

        // swiftlint:disable:next force_cast
        private lazy var player = VideoPlayer(layer: layer as! CAMetalLayer)

        init(feed: BrowserFeed) {
            super.init(frame: .zero)
            show(feed)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            player.resize(to: bounds.size, scale: traitCollection.displayScale)
        }

        func show(_ feed: BrowserFeed) {
            let player = player
            feed.show(on: self) { player.enqueue($0) }
        }
    }
#endif

/// Renders the local `CALayerHost` when available, with `BrowserVideoView` underneath as a
/// fallback still. While the layer is active, the video view matches the page's aspect ratio so a
/// previous-size still cannot peek out beside the layer.
struct BrowserPicture: View {
    @Environment(\.theme) private var theme
    let browser: CanvasBrowser
    let feed: BrowserFeed
    let blank: Bool

    /// Returns `nil` in video-only mode so the video view fits each picture itself, without lagging
    /// behind frame size changes by a SwiftUI layout pass.
    private var layerRatio: CGFloat? {
        feed.window.layer == nil ? nil : Self.ratio(of: feed.window.size)
    }

    private static func ratio(of size: CGSize) -> CGFloat? {
        size.width > 0 && size.height > 0 ? size.width / size.height : nil
    }

    var body: some View {
        GeometryReader { area in
            BrowserVideoView(feed: feed)
                .aspectRatio(layerRatio ?? Self.ratio(of: area.size), contentMode: .fit)
                .frame(width: area.size.width, height: area.size.height)
        }
        .opacity(feed.picture != .waiting && !blank ? 1 : 0)
        .grayscale(browser.state == .awake ? 0 : 1)
        .animation(theme.motion.fade, value: browser.state)
        #if os(macOS)
            if let layer = feed.window.layer, !blank {
                BrowserLayerView(context: layer, page: feed.window.size)
            }
        #endif
    }
}
