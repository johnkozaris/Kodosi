import CoreGraphics
import Foundation
import KodosiKit
import os

/// The runtime's calls for the video pipes that the app watches, on each pipe's own thread.
extension RuntimeHandle {
    static let onVideoFrame: kodosi_video_frame_cb_t = { pipe, frame, ud in
        autoreleasepool {
            guard let pipe, let frame, let runtime = resolve(ud) else { return Int32(KODOSI_VIDEO_TAKEN) }
            let given = frame.pointee
            let region = given.region.map { part in
                CGRect(x: CGFloat(part.pointee.x), y: CGFloat(part.pointee.y),
                       width: CGFloat(part.pointee.width), height: CGFloat(part.pointee.height))
            }
            let bytes = UnsafeRawBufferPointer(start: given.data, count: Int(given.length))
            guard let picture = VideoFrame(codec: given.codec, region: region, bytes: bytes),
                  runtime.videoPipes.frame(String(cString: pipe), picture)
            else { return Int32(KODOSI_VIDEO_FAILED) }
            return Int32(KODOSI_VIDEO_TAKEN)
        }
    }

    static let onVideoMessage: kodosi_video_message_cb_t = { pipe, json, len, ud in
        autoreleasepool {
            guard let pipe, let json, let runtime = resolve(ud) else { return }
            runtime.videoPipes.message(String(cString: pipe), Data(bytes: json, count: Int(len)))
        }
    }

    static let onVideoStale: kodosi_video_stale_cb_t = { pipe, stale, ud in
        autoreleasepool {
            guard let pipe, let runtime = resolve(ud) else { return }
            runtime.videoPipes.stale(String(cString: pipe), stale)
        }
    }
}

extension RuntimeHandle: VideoWatching {
    public func watch(_ pipe: String) {
        video(pipe, "watch") { kodosi_video_watch($0, $1) }
    }

    public func restart(_ pipe: String) {
        video(pipe, "restart") { kodosi_video_restart($0, $1) }
    }

    public func unwatch(_ pipe: String) {
        video(pipe, "unwatch") { kodosi_video_unwatch($0, $1) }
    }

    private func video(_ pipe: String, _ act: String, _ call: (UnsafeMutableRawPointer, UnsafePointer<CChar>) -> Int32) {
        guard let generation = currentGeneration() else { return }
        let result = withHandle(matching: generation) { handle in
            pipe.withCString { call(handle, $0) }
        }
        if let result, result != Int32(KODOSI_FFI_OK), result != Int32(KODOSI_FFI_SESSION_NOT_FOUND) {
            Logger.runtime.error("video \(act, privacy: .public) of a pipe failed: \(result)")
        }
    }
}
