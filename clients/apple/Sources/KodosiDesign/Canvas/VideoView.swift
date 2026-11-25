import KodosiBridge
import QuartzCore
import SwiftUI

/// A feed whose frames a `VideoSurface` shows.
@MainActor
protocol VideoFeed: AnyObject {
    func show(on surface: AnyObject, _ display: @escaping @Sendable (VideoSample) -> Bool)
}

extension BrowserFeed: VideoFeed {}
extension ScreenFeed: VideoFeed {}

#if os(macOS)
    /// A feed's video over the whole view. Without a placement, each picture fits the view whole.
    struct VideoView: NSViewRepresentable {
        let feed: any VideoFeed
        var placement: VideoPlacement?

        func makeNSView(context _: Context) -> VideoSurface {
            VideoSurface(feed: feed)
        }

        func updateNSView(_ view: VideoSurface, context _: Context) {
            view.show(feed)
            view.place(placement)
        }
    }

    final class VideoSurface: NSView {
        private let player = VideoPlayer(layer: CAMetalLayer())

        init(feed: any VideoFeed) {
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

        func show(_ feed: any VideoFeed) {
            let player = player
            feed.show(on: self) { player.enqueue($0) }
        }

        func place(_ placement: VideoPlacement?) {
            player.place(placement)
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

        func makeUIView(context _: Context) -> VideoSurface {
            VideoSurface(feed: feed)
        }

        func updateUIView(_ view: VideoSurface, context _: Context) {
            view.show(feed)
            view.place(placement)
        }
    }

    final class VideoSurface: UIView {
        override static var layerClass: AnyClass {
            CAMetalLayer.self
        }

        // swiftlint:disable:next force_cast
        private lazy var player = VideoPlayer(layer: layer as! CAMetalLayer)

        init(feed: any VideoFeed) {
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

        func show(_ feed: any VideoFeed) {
            let player = player
            feed.show(on: self) { player.enqueue($0) }
        }

        func place(_ placement: VideoPlacement?) {
            player.place(placement)
        }
    }
#endif
