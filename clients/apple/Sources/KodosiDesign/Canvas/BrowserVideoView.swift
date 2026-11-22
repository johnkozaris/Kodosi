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

/// AVFoundation marks its video renderer as not sendable, yet it takes sample buffers from any
/// queue, and the frames come on the task of the browser pipes.
private final class VideoRenderer: @unchecked Sendable {
    private let renderer: AVSampleBufferVideoRenderer

    init(_ renderer: AVSampleBufferVideoRenderer) {
        self.renderer = renderer
    }

    /// A display that lost the stream starts again at the next key frame.
    func enqueue(_ sample: CMSampleBuffer) -> Bool {
        let failed = renderer.status == .failed
        if failed {
            renderer.flush()
        }
        renderer.enqueue(sample)
        return !failed
    }
}

/// The page: the layer that the browser on this Mac draws, when the host gives it, or else the
/// video. Under the layer, the video view keeps the browser's last still for the time that the
/// layer cannot show the page. The video view has the page's shape, so a still from before a new
/// shape never shows beside the layer.
struct BrowserPicture: View {
    @Environment(\.theme) private var theme
    let browser: CanvasBrowser
    let feed: BrowserFeed
    let blank: Bool

    /// A video alone fills the view, and the display fits each picture in its own shape at once: a
    /// view that took the shape from the layout lagged the pictures by a frame, and while the page
    /// takes a new size its pictures still have the size before.
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
