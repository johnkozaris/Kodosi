import CoreGraphics
import Foundation
import Synchronization

/// What the runtime does with the app's watch of a video pipe.
public protocol VideoWatching: AnyObject, Sendable {
    /// The pipe's messages and the frames that its decoder can take come from now on.
    func watch(_ pipe: String)
    /// The decoder started over, so it needs a keyframe.
    func restart(_ pipe: String)
    /// Returns once no call for the pipe runs and none will come.
    func unwatch(_ pipe: String)
}

/// A frame that the decoder of a video pipe can take. `bytes`, its Annex B picture, is valid only
/// during the call that hands it over.
public struct VideoFrame {
    let codec: VideoCodec
    /// The part of the source that the frame shows, when only a part, in the source's points.
    let region: CGRect?
    let bytes: UnsafeRawBufferPointer

    public init?(codec: UInt8, region: CGRect?, bytes: UnsafeRawBufferPointer) {
        guard let codec = VideoCodec(rawValue: codec) else { return nil }
        self.codec = codec
        self.region = region
        self.bytes = bytes
    }
}

/// The video pipes (the shared browser's and the screen's) that this app watches. The runtime
/// hands each pipe's messages and frames over on a thread of that pipe, in pipe order, so they
/// never wait for the main actor.
public final class VideoPipes: Sendable {
    private struct Watched {
        weak var link: DocumentLink?
    }

    private struct State {
        var watched: [String: Watched] = [:]
        weak var runtime: any VideoWatching?
    }

    private let state = Mutex(State())

    public init() {}

    /// Hands the watches to `runtime`, which outlives none of them.
    public func attach(_ runtime: any VideoWatching) {
        state.withLock { $0.runtime = runtime }
    }

    func watch(_ pipe: String, by link: DocumentLink) {
        let runtime = state.withLock { state in
            state.watched[pipe] = Watched(link: link)
            return state.runtime
        }
        runtime?.watch(pipe)
    }

    func restart(_ pipe: String) {
        state.withLock { $0.runtime }?.restart(pipe)
    }

    func unwatch(_ pipe: String) {
        let runtime = state.withLock { state in
            state.watched[pipe] = nil
            return state.runtime
        }
        runtime?.unwatch(pipe)
    }

    /// Returns `false` when the decoder cannot go on.
    public func frame(_ pipe: String, _ frame: VideoFrame) -> Bool {
        link(pipe)?.frame(frame) ?? true
    }

    public func message(_ pipe: String, _ json: Data) {
        link(pipe)?.message(json)
    }

    /// True when the picture is not the stream's newest any more, false when it is again.
    public func stale(_ pipe: String, _ stale: Bool) {
        link(pipe)?.stale(stale)
    }

    private func link(_ pipe: String) -> DocumentLink? {
        state.withLock { $0.watched[pipe]?.link }
    }
}
