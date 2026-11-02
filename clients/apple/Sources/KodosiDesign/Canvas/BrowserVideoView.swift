import AVFoundation
import KodosiBridge
import SwiftUI

#if os(macOS)
    /// The live picture of the shared browser: each frame shows when it arrives.
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
            let video = video
            feed.show = { [weak feed] sample in
                Self.enqueue(sample, on: video, feed: feed)
            }
        }
    }

#else
    /// The live picture of the shared browser: each frame shows when it arrives.
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
            let video = video
            feed.show = { [weak feed] sample in
                Self.enqueue(sample, on: video, feed: feed)
            }
        }
    }
#endif

extension VideoSurface {
    /// A display that lost the stream starts again at the next key frame.
    @MainActor
    static func enqueue(_ sample: CMSampleBuffer, on video: AVSampleBufferDisplayLayer, feed: BrowserFeed?) {
        let renderer = video.sampleBufferRenderer
        if renderer.status == .failed {
            renderer.flush()
            feed?.restart()
        }
        renderer.enqueue(sample)
    }
}
