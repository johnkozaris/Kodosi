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
