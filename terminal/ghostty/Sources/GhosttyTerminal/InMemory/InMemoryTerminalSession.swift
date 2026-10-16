import Foundation
import GhosttyKit

public final class InMemoryTerminalSession: @unchecked Sendable {
    private static let slowSurfaceWriteThreshold: Duration = .milliseconds(500)

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
            hostResize: Self.resizeSurfaceToHost,
            restore: Self.restoreSurfaceCheckpoint
        )
    }

    init(
        write: @escaping @Sendable (Data) -> Void,
        resize: @escaping @Sendable (InMemoryTerminalViewport) -> Void,
        surfaceWrite: @escaping InMemoryTerminalSurfaceAccess.Write,
        surfaceHostResize: @escaping InMemoryTerminalSurfaceAccess.HostResize =
            InMemoryTerminalSession.resizeSurfaceToHost,
        surfaceRestore: @escaping InMemoryTerminalSurfaceAccess.Restore =
            InMemoryTerminalSession.restoreSurfaceCheckpoint
    ) {
        writeHandler = write
        resizeHandler = resize
        surfaceAccess = InMemoryTerminalSurfaceAccess(
            write: surfaceWrite,
            hostResize: surfaceHostResize,
            restore: surfaceRestore
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

    /// Gives the terminal the grid that the host gave its own, after the output
    /// that came before the resize, with the host's cell size so that images
    /// cover the same cells. The view's size never changes the grid.
    @discardableResult
    public func applyHostResize(_ viewport: InMemoryTerminalViewport) -> Bool {
        guard surfaceAccess.enqueueHostResize(viewport) else {
            TerminalDebugLog.log(
                .metrics,
                "host resize dropped cols=\(viewport.columns) rows=\(viewport.rows)"
            )
            return false
        }
        TerminalDebugLog.log(
            .metrics,
            "host resize queued cols=\(viewport.columns) rows=\(viewport.rows) cell=\(viewport.cellWidthPixels)x\(viewport.cellHeightPixels)"
        )
        return true
    }

    /// Replaces the terminal with a Ghostty snapshot. A console host keeps its
    /// scrollback on resize, and the snapshot does not hold that rule.
    @discardableResult
    public func restoreCheckpointSynchronously(_ data: Data, console: Bool) -> Bool {
        guard surfaceAccess.restoreCheckpointSynchronously(data, console: console) else {
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
            "view resize observed, the host sets the grid cols=\(cols) rows=\(rows) pixels=\(widthPx)x\(heightPx)"
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
        let duration = SuspendingClock().measure {
            data.withUnsafeBytes { buffer in
                guard let ptr = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return
                }
                ghostty_surface_write_buffer(surface, ptr, UInt(buffer.count))
            }
        }
        if duration >= slowSurfaceWriteThreshold {
            TerminalDebugLog.log(
                .output,
                "surface write slow bytes=\(data.count) duration=\(String(format: "%.3f", duration / .seconds(1)))s"
            )
        }
    }

    private static func resizeSurfaceToHost(
        _ surface: ghostty_surface_t,
        _ viewport: InMemoryTerminalViewport
    ) -> Bool {
        ghostty_surface_host_resize(
            surface,
            viewport.columns,
            viewport.rows,
            viewport.cellWidthPixels,
            viewport.cellHeightPixels
        ) == GHOSTTY_RESULT_SUCCESS
    }

    private static func restoreSurfaceCheckpoint(
        _ surface: ghostty_surface_t,
        _ data: Data,
        _ console: Bool
    ) -> Bool {
        data.withUnsafeBytes { buffer in
            ghostty_surface_checkpoint_restore(
                surface,
                buffer.baseAddress?.assumingMemoryBound(to: UInt8.self),
                buffer.count,
                !console
            ) == GHOSTTY_RESULT_SUCCESS
        }
    }
}
