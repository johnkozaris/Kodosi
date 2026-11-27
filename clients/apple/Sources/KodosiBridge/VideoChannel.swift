import CoreGraphics
import CoreMedia
import CoreVideo
import Foundation
import Synchronization
import VideoToolbox

/// How far a view of a video is.
public enum VideoPicture: Sendable {
    case waiting
    case live
    /// Retains the last rendered frame while reconnecting until a fresh frame arrives.
    case kept

    /// A live picture rests while it is not the stream's newest, and a kept one is live again once
    /// it is.
    func stale(_ stale: Bool) -> Self {
        switch (self, stale) {
        case (.live, true): .kept
        case (.kept, false): .live
        default: self
        }
    }
}

/// The one state that a view of a canvas video shows, on its picture. Kodosi handles what comes
/// back by itself, so only `away` says why and only `failed` offers to try again.
public enum VideoShowing: Equatable, Sendable {
    case away(VideoAway)
    /// Something unexpected broke.
    case failed
    case live
    /// The last picture, dimmed while the video comes back.
    case resting
    /// No picture yet.
    case opening
}

/// Why a video shows nothing for now. Each clears by itself once its cause does.
public enum VideoAway: Equatable, Sendable {
    case sharingOff
    case offline
    case locked
    case anotherAccount
    case asleep
    case unrecorded
}

/// The last picture that a feed showed, out of its decoder's pool, which a new view of the feed
/// shows resting until the video is live again.
public struct VideoStill {
    public let picture: CVPixelBuffer
    /// The part of the source that the picture shows, when only a part.
    public let region: CGRect?

    public init(picture: CVPixelBuffer, region: CGRect?) {
        self.picture = picture
        self.region = region
    }
}

/// A frame on its way to a surface, with the part of the source that it shows when it shows only
/// a part, in the source's points.
public struct VideoSample {
    public let buffer: CMSampleBuffer
    public let region: CGRect?
}

/// Shows a feed's frames on a surface. The feed never makes two calls to it at once.
public protocol VideoDisplay: Sendable {
    /// Returns `false` if the renderer failed and needs a fresh keyframe.
    func enqueue(_ sample: VideoSample) -> Bool
    /// The watch ended, so the decoder can go. The next watch starts at a keyframe.
    func stop()
}

/// A feed's pipe. The runtime hands its JSON messages and the frames that the decoder can take
/// over on the pipe's own thread, in pipe order, so a busy main thread never holds them up; it
/// acknowledges each frame once the display took it. Everything reaches the feed in pipe order.
final class VideoChannel<Message: Decodable & Sendable>: Sendable {
    enum Heard: Sendable {
        case news(DocumentLink.News)
        case message(Message)
        /// The surface generation that rendered a frame.
        case shown(Int)
        /// The part of the source that the frames show from now on, or `nil` for all of it.
        case region(CGRect?)
        /// True: the picture is not the stream's newest, so it is no longer live until it is again
        /// or the next frame comes. False: it is the newest again.
        case stale(Bool)
    }

    private struct State {
        var link: DocumentLink?
        var teller: AsyncStream<Heard>.Continuation?
        var display: any VideoDisplay?
        var surface = 0
        var shown = false
        var region: CGRect?
    }

    private let state = Mutex(State())
    /// Only the pipe's thread builds and decodes samples, so the main thread never waits for one.
    private let samples = Mutex(VideoSamples())

    /// Hears `link` until `stop`, unless another watch already does.
    func start(_ link: DocumentLink) -> AsyncStream<Heard>? {
        let (heard, teller) = AsyncStream.makeStream(of: Heard.self)
        let starts = state.withLock { state in
            guard state.link == nil else { return false }
            state.link = link
            state.teller = teller
            return true
        }
        guard starts else { return nil }
        link.onNews = { [weak self] in self?.hear($0) }
        link.video = VideoCalls(
            frame: { [weak self] in self?.frame($0) ?? true },
            message: { [weak self] in self?.receive($0) },
            stale: { [weak self] in self?.stale($0) },
        )
        return heard
    }

    /// The runtime makes no call for the pipe once its link closed, so the display stops after it.
    func stop() {
        let (link, teller) = state.withLock { state in
            defer {
                state.link = nil
                state.teller = nil
                state.shown = false
            }
            return (state.link, state.teller)
        }
        link?.close()
        state.withLock { $0.display }?.stop()
        teller?.finish()
    }

    /// Hands frames to `display` from now on. A new surface number shows a frame before it counts
    /// as shown.
    func show(on surface: Int, _ display: any VideoDisplay) {
        state.withLock { state in
            state.display = display
            if state.surface != surface {
                state.surface = surface
                state.shown = false
            }
        }
    }

    /// Lets go of the display of a surface that went away.
    func hide() {
        state.withLock { $0.display = nil }
    }

    /// The decoder starts over, so the runtime waits for a keyframe and asks for one.
    func restart() {
        state.withLock { $0.link }?.restartVideo()
    }

    /// The next frame counts as shown again, after a while without frames.
    func rest() {
        state.withLock { $0.shown = false }
    }

    func post(_ message: some Encodable) {
        guard let body = try? JSONEncoder().encode(message) else { return }
        state.withLock { $0.link }?.send(Data([0]) + body)
    }

    private func receive(_ json: Data) {
        if let decoded = try? JSONDecoder().decode(Message.self, from: json) {
            tell(.message(decoded))
        }
    }

    private func hear(_ news: DocumentLink.News) {
        if case .lost = news {
            state.withLock { $0.shown = false }
        }
        tell(.news(news))
    }

    private func stale(_ stale: Bool) {
        if stale {
            state.withLock { $0.shown = false }
        }
        tell(.stale(stale))
    }

    private func tell(_ heard: Heard) {
        state.withLock { $0.teller }?.yield(heard)
    }

    /// Returns `false` when the decoder cannot go on. The display decodes outside the state's lock,
    /// so the main thread that sends input never waits for it.
    private func frame(_ frame: VideoFrame) -> Bool {
        let (display, surface) = state.withLock { ($0.display, $0.surface) }
        let decoded = samples.withLock { samples -> Bool? in
            guard let sample = samples.sample(of: frame.bytes, codec: frame.codec) else { return nil }
            return display?.enqueue(VideoSample(buffer: sample, region: frame.region)) ?? true
        }
        guard let going = decoded else { return false }
        let told = state.withLock { state -> [Heard] in
            // A frame still on its way when the watch stopped tells nobody.
            guard state.link != nil else { return [] }
            var told: [Heard] = []
            if frame.region != state.region {
                state.region = frame.region
                told.append(.region(frame.region))
            }
            if going, !state.shown, state.surface == surface {
                state.shown = true
                told.append(.shown(surface))
            }
            return told
        }
        for heard in told {
            tell(heard)
        }
        return going
    }
}

/// A codec that this device decodes, which the host picks from.
struct DeviceDecoder: Encodable {
    static let here = [
        DeviceDecoder(codec: "h264", hardware: VTIsHardwareDecodeSupported(kCMVideoCodecType_H264)),
        DeviceDecoder(codec: "hevc", hardware: VTIsHardwareDecodeSupported(kCMVideoCodecType_HEVC)),
    ]

    let codec: String
    let hardware: Bool
}

/// The size of a view in points and the pixels per point that it asks for.
struct VideoSize: Encodable, Equatable {
    let width: Int
    let height: Int
    let scale: Double

    /// A hot device asks for half of its pixels per point, though never for fewer than one.
    init(_ size: CGSize, scale: CGFloat, hot: Bool) {
        width = Int(size.width)
        height = Int(size.height)
        self.scale = Double(hot ? max(1, scale / 2) : scale)
    }
}
