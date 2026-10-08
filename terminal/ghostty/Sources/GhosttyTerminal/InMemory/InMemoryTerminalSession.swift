import Foundation
import GhosttyKit

public final class InMemoryTerminalSession: @unchecked Sendable {
    private static let slowSurfaceWriteThreshold: TimeInterval = 0.5

    private let resizeLock = NSLock()
    private let surfaceAccess: InMemoryTerminalSurfaceAccess
    private var lastResize: InMemoryTerminalViewport?
    private let writeHandler: @Sendable (Data) -> Void
    private let resizeHandler: @Sendable (InMemoryTerminalViewport) -> Void

    public init(
        write: @escaping @Sendable (Data) -> Void,
        resize: @escaping @Sendable (InMemoryTerminalViewport) -> Void
    ) {
        writeHandler = write
        resizeHandler = resize
        surfaceAccess = InMemoryTerminalSurfaceAccess(
            write: Self.writeToSurface,
            restore: Self.restoreSurfaceCheckpoint,
            processExit: Self.reportProcessExit
        )
    }

    init(
        write: @escaping @Sendable (Data) -> Void,
        resize: @escaping @Sendable (InMemoryTerminalViewport) -> Void,
        surfaceWrite: @escaping InMemoryTerminalSurfaceAccess.Write,
        surfaceRestore: @escaping InMemoryTerminalSurfaceAccess.Restore =
            InMemoryTerminalSession.restoreSurfaceCheckpoint,
        processExit: @escaping InMemoryTerminalSurfaceAccess.ProcessExit =
            InMemoryTerminalSession.reportProcessExit
    ) {
        writeHandler = write
        resizeHandler = resize
        surfaceAccess = InMemoryTerminalSurfaceAccess(
            write: surfaceWrite,
            restore: surfaceRestore,
            processExit: processExit
        )
    }

    func setSurface(_ surface: ghostty_surface_t?) {
        surfaceAccess.setSurface(surface)
        TerminalDebugLog.log(
            .lifecycle,
            "in-memory session surface=\(surface == nil ? "nil" : "set")"
        )
    }

    func clearSurface(ifMatches expectedSurface: ghostty_surface_t?) {
        guard surfaceAccess.clearSurface(ifMatches: expectedSurface) else {
            TerminalDebugLog.log(
                .lifecycle,
                "in-memory session clear skipped expected=\(expectedSurface == nil ? "nil" : "set") current=\(surfaceAccess.currentSurface == nil ? "nil" : "set")"
            )
            return
        }

        TerminalDebugLog.log(.lifecycle, "in-memory session surface=nil matched")
    }

    var currentSurface: ghostty_surface_t? {
        surfaceAccess.currentSurface
    }

    public func readViewportText() -> String? {
        surfaceAccess.withCurrentSurface { surface in
            let topLeft = ghostty_point_s(
                tag: GHOSTTY_POINT_VIEWPORT,
                coord: GHOSTTY_POINT_COORD_TOP_LEFT,
                x: 0,
                y: 0
            )
            let bottomRight = ghostty_point_s(
                tag: GHOSTTY_POINT_VIEWPORT,
                coord: GHOSTTY_POINT_COORD_BOTTOM_RIGHT,
                x: 0,
                y: 0
            )
            let selection = ghostty_selection_s(
                top_left: topLeft,
                bottom_right: bottomRight,
                rectangle: false
            )

            var out = ghostty_text_s()
            guard ghostty_surface_read_text(surface, selection, &out) else {
                return nil
            }
            defer { ghostty_surface_free_text(surface, &out) }

            guard let textPtr = out.text, out.text_len > 0 else {
                return ""
            }
            let bytes = UnsafeBufferPointer(start: textPtr, count: Int(out.text_len))
                .map { UInt8(bitPattern: $0) }
            return String(decoding: bytes, as: UTF8.self)
        } ?? nil
    }

    func updateViewport(_ size: TerminalGridMetrics) {
        TerminalDebugLog.log(.metrics, "in-memory viewport update \(size.debugSummary)")
        dispatchResize(InMemoryTerminalViewport(
            columns: size.columns,
            rows: size.rows,
            widthPixels: size.widthPixels,
            heightPixels: size.heightPixels,
            cellWidthPixels: size.cellWidthPixels,
            cellHeightPixels: size.cellHeightPixels
        ))
    }

    @discardableResult
    public func receive(_ data: Data) -> Bool {
        guard surfaceAccess.enqueueWrite(data) else {
            TerminalDebugLog.log(
                .output,
                "terminal <- host dropped \(TerminalDebugLog.describe(data))"
            )
            return false
        }

        TerminalDebugLog.log(
            .output,
            "terminal <- host \(TerminalDebugLog.describe(data))"
        )
        return true
    }

    @discardableResult
    public func restoreCheckpointSynchronously(_ data: Data) -> Bool {
        guard surfaceAccess.restoreCheckpointSynchronously(data) else {
            TerminalDebugLog.log(
                .output,
                "terminal checkpoint dropped \(TerminalDebugLog.describe(data))"
            )
            return false
        }
        TerminalDebugLog.log(
            .output,
            "terminal checkpoint queued \(TerminalDebugLog.describe(data))"
        )
        return true
    }

    @discardableResult
    public func receive(_ string: String) -> Bool {
        guard let data = string.data(using: .utf8) else { return false }
        return receive(data)
    }

    public func sendInput(_ data: Data) {
        TerminalDebugLog.log(
            .input,
            "host <- direct input \(TerminalDebugLog.describe(data))"
        )
        writeHandler(data)
    }

    public func finish(exitCode: UInt32, runtimeMilliseconds: UInt64) {
        guard surfaceAccess.enqueueProcessExit(
            exitCode: exitCode,
            runtimeMilliseconds: runtimeMilliseconds
        ) else {
            TerminalDebugLog.log(
                .lifecycle,
                "process exit ignored: missing surface exitCode=\(exitCode) runtimeMs=\(runtimeMilliseconds)"
            )
            return
        }

        TerminalDebugLog.log(
            .lifecycle,
            "process exit exitCode=\(exitCode) runtimeMs=\(runtimeMilliseconds)"
        )
    }

    static let receiveBufferCallback: ghostty_surface_receive_buffer_cb = { userdata, ptr, len in
        guard let userdata, let ptr,
              len <= InMemoryTerminalSurfaceAccess.maximumPendingWriteBytes
        else {
            TerminalDebugLog.log(.input, "terminal callback payload rejected bytes=\(len)")
            return
        }
        let session = Unmanaged<InMemoryTerminalSession>
            .fromOpaque(userdata)
            .takeUnretainedValue()
        let data = Data(bytes: ptr, count: len)
        TerminalDebugLog.log(
            .input,
            "host <- terminal \(TerminalDebugLog.describe(data))"
        )
        session.writeHandler(data)
    }

    static let receiveResizeCallback: ghostty_surface_receive_resize_cb = { userdata, cols, rows, widthPx, heightPx in
        guard userdata != nil else { return }
        TerminalDebugLog.log(
            .metrics,
            "backend resize observed but platform viewport remains authoritative cols=\(cols) rows=\(rows) pixels=\(widthPx)x\(heightPx)"
        )
    }

    private func dispatchResize(_ resize: InMemoryTerminalViewport) {
        resizeLock.lock()
        let mergedResize = mergedResize(resize)
        guard mergedResize != lastResize else {
            resizeLock.unlock()
            TerminalDebugLog.log(
                .metrics,
                "resize unchanged cols=\(mergedResize.columns) rows=\(mergedResize.rows) pixels=\(mergedResize.widthPixels)x\(mergedResize.heightPixels) cell=\(mergedResize.cellWidthPixels)x\(mergedResize.cellHeightPixels)"
            )
            return
        }
        lastResize = mergedResize
        resizeLock.unlock()

        TerminalDebugLog.log(
            .metrics,
            "resize dispatched cols=\(mergedResize.columns) rows=\(mergedResize.rows) pixels=\(mergedResize.widthPixels)x\(mergedResize.heightPixels) cell=\(mergedResize.cellWidthPixels)x\(mergedResize.cellHeightPixels)"
        )
        resizeHandler(mergedResize)
    }

    private func mergedResize(_ resize: InMemoryTerminalViewport) -> InMemoryTerminalViewport {
        guard let lastResize else { return resize }

        return InMemoryTerminalViewport(
            columns: resize.columns,
            rows: resize.rows,
            widthPixels: resize.widthPixels == 0 ? lastResize.widthPixels : resize.widthPixels,
            heightPixels: resize.heightPixels == 0 ? lastResize.heightPixels : resize.heightPixels,
            cellWidthPixels: resize.cellWidthPixels == 0 ? lastResize.cellWidthPixels : resize.cellWidthPixels,
            cellHeightPixels: resize.cellHeightPixels == 0 ? lastResize.cellHeightPixels : resize.cellHeightPixels
        )
    }

    func restoreFromOutputQueueForTesting(_ data: Data) -> Bool {
        surfaceAccess.attemptRestoreFromOutputQueueForTesting(data)
    }

    func waitForPendingOutput() {
        surfaceAccess.waitForPendingOutput()
    }

    private static func writeToSurface(_ surface: ghostty_surface_t, _ data: Data) {
        let start = ProcessInfo.processInfo.systemUptime
        defer {
            let duration = ProcessInfo.processInfo.systemUptime - start
            if duration >= slowSurfaceWriteThreshold {
                TerminalDebugLog.log(
                    .output,
                    "surface write slow bytes=\(data.count) duration=\(String(format: "%.3f", duration))s"
                )
            }
        }

        data.withUnsafeBytes { buffer in
            guard let ptr = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return
            }
            ghostty_surface_write_buffer(surface, ptr, UInt(buffer.count))
        }
    }

    private static func restoreSurfaceCheckpoint(
        _ surface: ghostty_surface_t,
        _ data: Data
    ) -> Bool {
        var options = ghostty_checkpoint_restore_options_s(
            size: MemoryLayout<ghostty_checkpoint_restore_options_s>.size,
            limits: ghostty_checkpoint_limits_s(
                size: MemoryLayout<ghostty_checkpoint_limits_s>.size,
                max_json_bytes: 8 * 1024 * 1024,
                max_string_bytes: 8 * 1024 * 1024,
                max_continuation_bytes: 1024 * 1024,
                max_cells: 4096 * 4096
            )
        )
        return data.withUnsafeBytes { buffer in
            ghostty_surface_checkpoint_restore(
                surface,
                buffer.baseAddress?.assumingMemoryBound(to: UInt8.self),
                buffer.count,
                &options,
                nil
            ) == GHOSTTY_RESULT_SUCCESS
        }
    }

    private static func reportProcessExit(
        _ surface: ghostty_surface_t,
        _ exitCode: UInt32,
        _ runtimeMilliseconds: UInt64
    ) {
        ghostty_surface_process_exit(surface, exitCode, runtimeMilliseconds)
    }
}
