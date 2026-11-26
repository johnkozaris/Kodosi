import KodosiBridge
import QuartzCore
import SwiftUI

/// A feed whose frames a `VideoSurface` shows. Its still picture outlives the surface, so that
/// the next view of the feed shows it until the feed is live again.
@MainActor
protocol VideoFeed: AnyObject {
    var still: VideoStill? { get set }
    func show(on surface: AnyObject, _ display: any VideoDisplay)
    func hide(from surface: AnyObject) -> Bool
}

extension BrowserFeed: VideoFeed {}
extension ScreenFeed: VideoFeed {}

#if os(macOS)
    /// A feed's video over the whole view. Without a placement, each picture fits the view whole.
    struct VideoView: NSViewRepresentable {
        let feed: any VideoFeed
        var placement: VideoPlacement?
        var resting = false

        func makeNSView(context _: Context) -> VideoSurface {
            VideoSurface(feed: feed)
        }

        func updateNSView(_ view: VideoSurface, context _: Context) {
            view.show(feed)
            view.place(placement, resting: resting)
        }

        static func dismantleNSView(_ view: VideoSurface, coordinator _: ()) {
            view.leave()
        }
    }

    final class VideoSurface: NSView {
        private let player = VideoPlayer(layer: CAMetalLayer())
        private(set) weak var feed: any VideoFeed?

        init(feed: any VideoFeed) {
            super.init(frame: .zero)
            layer = player.layer
            wantsLayer = true
            show(feed)
            player.seed(feed.still)
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

        func show(_ feed: any VideoFeed) {
            self.feed = feed
            feed.show(on: self, player)
        }

        func place(_ placement: VideoPlacement?, resting: Bool) {
            player.place(placement)
            player.rest(resting)
        }

        /// The feed keeps the picture on screen when this view goes.
        func leave() {
            if feed?.hide(from: self) == true, let still = player.still() {
                feed?.still = still
            }
        }

        private func fit() {
            player.resize(to: bounds.size, scale: window?.backingScaleFactor ?? 1)
        }
    }

#else
    /// A feed's video over the whole view. Without a placement, each picture fits the view whole.
    struct VideoView: UIViewRepresentable {
        let feed: any VideoFeed
        var placement: VideoPlacement?
        var resting = false

        func makeUIView(context _: Context) -> VideoSurface {
            VideoSurface(feed: feed)
        }

        func updateUIView(_ view: VideoSurface, context _: Context) {
            view.show(feed)
            view.place(placement, resting: resting)
        }

        static func dismantleUIView(_ view: VideoSurface, coordinator _: ()) {
            view.leave()
        }
    }

    final class VideoSurface: UIView {
        override static var layerClass: AnyClass {
            CAMetalLayer.self
        }

        // swiftlint:disable:next force_cast
        private lazy var player = VideoPlayer(layer: layer as! CAMetalLayer)
        private(set) weak var feed: any VideoFeed?

        init(feed: any VideoFeed) {
            super.init(frame: .zero)
            show(feed)
            player.seed(feed.still)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            player.resize(to: bounds.size, scale: traitCollection.displayScale)
        }

        func show(_ feed: any VideoFeed) {
            self.feed = feed
            feed.show(on: self, player)
        }

        func place(_ placement: VideoPlacement?, resting: Bool) {
            player.place(placement)
            player.rest(resting)
        }

        /// The feed keeps the picture on screen when this view goes.
        func leave() {
            if feed?.hide(from: self) == true, let still = player.still() {
                feed?.still = still
            }
        }
    }
#endif
