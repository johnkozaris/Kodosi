import Foundation
import KodosiBridge
import KodosiKit
import KodosiTerminal
import os

struct TerminalResizeContext {
    let sessionId: String
    let commandSink: CommandSink
    let lastResize: ResizeDeliveryState
    let resizeAuthority: TerminalResizeAuthorityCell
    let token: TerminalConnectionToken
    let runtimeIncarnationId: String
    let surfaceGeneration: UInt64

    func deliver(_ viewport: TerminalViewport, claim: Bool) {
        let cols = Int(viewport.columns)
        let rows = Int(viewport.rows)
        guard resizeAuthority.isAllowed,
              token.isActive,
              cols > 0, rows > 0,
              let wireCols = UInt16(exactly: cols),
              let wireRows = UInt16(exactly: rows),
              let pixelGeometry = viewport.pixelGeometry
        else { return }
        let key = [
            "\(cols)x\(rows)",
            "\(pixelGeometry.width)x\(pixelGeometry.height)",
            "\(pixelGeometry.cellWidth)x\(pixelGeometry.cellHeight)",
            "surface:\(surfaceGeneration)",
        ].joined(separator: ":")
        guard !runtimeIncarnationId.isEmpty else { return }
        let measuredIdentity = TerminalResizeIdentity(
            requestId: UUIDv7.generate(),
            expectedRuntimeIncarnationId: runtimeIncarnationId,
            subscriptionId: token.subscriptionId,
            subscriptionGeneration: token.subscriptionGeneration,
            surfaceGeneration: surfaceGeneration,
            cols: wireCols,
            rows: wireRows,
            widthPixels: pixelGeometry.width,
            heightPixels: pixelGeometry.height,
            cellWidthPixels: pixelGeometry.cellWidth,
            cellHeightPixels: pixelGeometry.cellHeight
        )
        guard let identity = lastResize.begin(measuredIdentity, key: key) else { return }

        let signposter = PerformanceSignposts.terminal
        let signpostID = signposter.makeSignpostID()
        let interval = signposter.beginInterval(
            "resize.deliver",
            id: signpostID,
            "cols=\(wireCols) rows=\(wireRows)"
        )
        Task { @MainActor in
            guard token.isActive else {
                lastResize.reset()
                signposter.endInterval("resize.deliver", interval, "queued=0")
                return
            }
            commandSink.sendTerminalResize(
                sessionId: sessionId,
                identity: identity,
                claim: claim
            ) { result in
                let queued = result == Int32(KODOSI_FFI_OK)
                lastResize.queued(identity, accepted: queued)
                signposter.endInterval(
                    "resize.deliver",
                    interval,
                    "queued=\(queued ? 1 : 0, privacy: .public)"
                )
            }
        }
    }
}

extension TerminalViewport {
    struct PixelGeometry: Equatable, Sendable {
        let width: UInt32
        let height: UInt32
        let cellWidth: UInt32
        let cellHeight: UInt32
    }

    var pixelGeometry: PixelGeometry? {
        guard cellWidthPixels > 0,
              cellHeightPixels > 0
        else { return nil }
        return PixelGeometry(
            width: UInt32(columns) * cellWidthPixels,
            height: UInt32(rows) * cellHeightPixels,
            cellWidth: cellWidthPixels,
            cellHeight: cellHeightPixels
        )
    }
}
