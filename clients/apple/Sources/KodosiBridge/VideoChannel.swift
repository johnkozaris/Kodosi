import CoreGraphics
import CoreMedia
import Foundation
import Synchronization
import VideoToolbox

/// How far a view of a video is.
public enum VideoPicture: Sendable {
    case waiting
    case live
    /// Retains the last rendered frame while reconnecting until a fresh frame arrives.
    case kept
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

/// A feed's pipe: JSON messages after a 0 byte, and video frames after a 1 byte. Frames are
/// acknowledged, decoded and shown on the `VideoPipes` task so a busy main thread never triggers
/// host backpressure; per-frame acks prevent queueing and let the host match quality to the link.
/// Everything else reaches the feed in pipe order.
final class VideoChannel<Message: Decodable & Sendable>: Sendable {
    enum Heard: Sendable {
        case news(DocumentLink.News)
        case message(Message)
        /// The surface generation that rendered a frame.
        case shown(Int)
    }

    private struct State {
        var link: DocumentLink?
        var teller: AsyncStream<Heard>.Continuation?
        var display: any VideoDisplay?
        var surface = 0
        var shown = false
        var samples = VideoSamples()
        var lastFrame: UInt32 = 0
        var holds = Holds.nothing
    }

    private let state = Mutex(State())

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
        link.onFrame = { [weak self] in self?.receive($0) }
        link.onNews = { [weak self] in self?.hear($0) }
        return heard
    }

    func stop() {
        let (link, teller) = state.withLock { state in
            defer {
                state.link = nil
                state.teller = nil
                state.shown = false
            }
            state.display?.stop()
            return (state.link, state.teller)
        }
        teller?.finish()
        link?.close()
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

    /// The decoder starts over, so it waits for the keyframe that it asks for.
    func restart() {
        rejoin()
        post(Keyframe())
    }

    /// A new watch starts with a keyframe.
    func rejoin() {
        state.withLock { $0.holds = .nothing }
    }

    func post(_ message: some Encodable) {
        guard let body = try? JSONEncoder().encode(message) else { return }
        state.withLock { $0.link }?.send(Data([Wire.messageTag]) + body)
    }

    private func receive(_ message: Data) {
        switch message.first {
        case Wire.frameTag:
            frame(message)
        case Wire.messageTag:
            if let decoded = try? JSONDecoder().decode(Message.self, from: message.dropFirst()) {
                tell(.message(decoded))
            }
        default:
            break
        }
    }

    private func hear(_ news: DocumentLink.News) {
        if case .lost = news {
            state.withLock { $0.shown = false }
        }
        tell(.news(news))
    }

    private func tell(_ heard: Heard) {
        state.withLock { $0.teller }?.yield(heard)
    }

    /// The host intentionally skips frames for slow viewers and resumes on an anchor frame, so a
    /// sequence gap only requires a keyframe request if the next frame is neither a keyframe nor
    /// an anchor.
    private func frame(_ message: Data) {
        guard message.count > Wire.frameHeader else { return }
        let header = [UInt8](message.prefix(Wire.frameHeader))
        let id = header[1 ..< 5].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }
        let key = header[5] & Wire.keyFlag != 0
        let anchor = header[5] & Wire.anchorFlag != 0
        post(Ack(frame: id))
        guard let codec = VideoCodec(rawValue: header[6]),
              let (region, data) = Self.picture(message.dropFirst(Wire.frameHeader), regional: header[5] & Wire.regionFlag != 0)
        else { return }
        let (restarts, shown) = state.withLock { state -> (Bool, Heard?) in
            // A frame still on its way when the watch stopped would start the stopped decoder again.
            guard state.link != nil else { return (false, nil) }
            if state.holds == .stream, id != state.lastFrame &+ 1 {
                state.holds = .gap
            }
            state.lastFrame = id
            switch state.holds {
            case .nothing where !key:
                return (false, nil)
            case .gap where !key && !anchor:
                return (true, nil)
            default:
                break
            }
            guard let sample = state.samples.sample(of: data, codec: codec) else {
                return (false, nil)
            }
            state.holds = .stream
            let going = state.display?.enqueue(VideoSample(buffer: sample, region: region)) ?? true
            defer { state.shown = true }
            return (!going, state.shown ? nil : .shown(state.surface))
        }
        if restarts {
            restart()
        }
        if let shown {
            tell(shown)
        }
    }

    /// Splits a frame's body into the region that it shows, four big-endian 32-bit floats (x, y,
    /// width and height) when it has one, and its Annex B data.
    private static func picture(_ body: Data, regional: Bool) -> (CGRect?, Data)? {
        guard regional else { return (nil, body) }
        guard body.count > Wire.regionBytes else { return nil }
        let values = stride(from: body.startIndex, to: body.startIndex + Wire.regionBytes, by: 4).map { start in
            CGFloat(Float(bitPattern: body[start ..< start + 4].reduce(UInt32(0)) { $0 << 8 | UInt32($1) }))
        }
        return (CGRect(x: values[0], y: values[1], width: values[2], height: values[3]), body.dropFirst(Wire.regionBytes))
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

/// A frame is a tag byte, its number, its flags and its codec, then the region it shows when its
/// flags say so, then its Annex B data.
private enum Wire {
    static let frameHeader = 7
    static let regionBytes = 16
    static let messageTag: UInt8 = 0
    static let frameTag: UInt8 = 1
    static let keyFlag: UInt8 = 1
    static let anchorFlag: UInt8 = 2
    static let regionFlag: UInt8 = 4
}

private struct Ack: Encodable {
    let type = "ack"
    let frame: UInt32
}

private struct Keyframe: Encodable {
    let type = "keyframe"
}

private enum Holds {
    /// Decoder has no reference state yet and must wait for a keyframe.
    case nothing
    /// Decoder is receiving contiguous frames since the last keyframe or anchor.
    case stream
    /// A sequence gap occurred; the next frame must be a keyframe or anchor.
    case gap
}
