import AVFoundation
import KodosiBridge
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
        private let video = AVSampleBufferDisplayLayer()

        init(feed: BrowserFeed) {
            super.init(frame: .zero)
            video.videoGravity = .resizeAspect
            layer = video
            wantsLayer = true
            show(feed)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        func show(_ feed: BrowserFeed) {
            let renderer = VideoRenderer(video.sampleBufferRenderer)
            feed.show(on: self) { renderer.enqueue($0) }
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
            AVSampleBufferDisplayLayer.self
        }

        private var video: AVSampleBufferDisplayLayer {
            // swiftlint:disable:next force_cast
            layer as! AVSampleBufferDisplayLayer
        }

        init(feed: BrowserFeed) {
            super.init(frame: .zero)
            video.videoGravity = .resizeAspect
            show(feed)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        func show(_ feed: BrowserFeed) {
            let renderer = VideoRenderer(video.sampleBufferRenderer)
            feed.show(on: self) { renderer.enqueue($0) }
        }
    }
#endif

/// Wraps `AVSampleBufferVideoRenderer` as `@unchecked Sendable`; AVFoundation accepts sample
/// buffers from any thread, and frames arrive on the `BrowserPipes` task.
private final class VideoRenderer: @unchecked Sendable {
    private let renderer: AVSampleBufferVideoRenderer

    init(_ renderer: AVSampleBufferVideoRenderer) {
        self.renderer = renderer
    }

    /// Flushes a failed renderer and returns `false` so the feed requests a fresh keyframe.
    func enqueue(_ sample: CMSampleBuffer) -> Bool {
        let failed = renderer.status == .failed
        if failed {
            renderer.flush()
        }
        renderer.enqueue(sample)
        return !failed
    }
}

/// Renders the local `CALayerHost` when available, with `BrowserVideoView` underneath as a
/// fallback still. While the layer is active, the video view matches the page's aspect ratio so a
/// previous-size still cannot peek out beside the layer.
struct BrowserPicture: View {
    @Environment(\.theme) private var theme
    let browser: CanvasBrowser
    let feed: BrowserFeed
    let blank: Bool

    /// Returns `nil` in video-only mode so `AVSampleBufferDisplayLayer` handles aspect-fitting
    /// directly without lagging behind frame size changes by a SwiftUI layout pass.
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
