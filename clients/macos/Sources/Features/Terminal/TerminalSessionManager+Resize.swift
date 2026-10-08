import Foundation
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
}

extension TerminalSessionManager {
    nonisolated struct RebasedPixelGeometry: Equatable, Sendable {
        let width: UInt32
        let height: UInt32
        let cellWidth: UInt32
        let cellHeight: UInt32
    }

    nonisolated static func exactPixelGeometry(
        viewport: TerminalViewport
    ) -> RebasedPixelGeometry? {
        guard viewport.cellWidthPixels > 0,
              viewport.cellHeightPixels > 0
        else { return nil }
        return RebasedPixelGeometry(
            width: UInt32(viewport.columns) * viewport.cellWidthPixels,
            height: UInt32(viewport.rows) * viewport.cellHeightPixels,
            cellWidth: viewport.cellWidthPixels,
            cellHeight: viewport.cellHeightPixels
        )
    }

    nonisolated static func handleTerminalResize(
        _ viewport: TerminalViewport,
        context: TerminalResizeContext,
        claim: Bool
    ) {
        let cols = Int(viewport.columns)
        let rows = Int(viewport.rows)
        guard context.resizeAuthority.isAllowed,
              context.token.isActive,
              cols > 0, rows > 0,
              let wireCols = UInt16(exactly: cols),
              let wireRows = UInt16(exactly: rows),
              let pixelGeometry = exactPixelGeometry(viewport: viewport)
        else { return }
        let key = [
            "\(cols)x\(rows)",
            "\(pixelGeometry.width)x\(pixelGeometry.height)",
            "\(pixelGeometry.cellWidth)x\(pixelGeometry.cellHeight)",
            "surface:\(context.surfaceGeneration)",
        ].joined(separator: ":")
        guard !context.runtimeIncarnationId.isEmpty else { return }
        let measuredIdentity = TerminalResizeIdentity(
            requestId: UUIDv7.generate(),
            expectedRuntimeIncarnationId: context.runtimeIncarnationId,
            subscriptionId: context.token.subscriptionId,
            subscriptionGeneration: context.token.subscriptionGeneration,
            surfaceGeneration: context.surfaceGeneration,
            cols: wireCols,
            rows: wireRows,
            widthPixels: pixelGeometry.width,
            heightPixels: pixelGeometry.height,
            cellWidthPixels: pixelGeometry.cellWidth,
            cellHeightPixels: pixelGeometry.cellHeight
        )
        guard let identity = context.lastResize.begin(measuredIdentity, key: key) else { return }

        let signposter = PerformanceSignposts.terminal
        let signpostID = signposter.makeSignpostID()
        let interval = signposter.beginInterval(
            "resize.deliver",
            id: signpostID,
            "cols=\(wireCols) rows=\(wireRows)"
        )
        Task { @MainActor in
            guard context.token.isActive else {
                context.lastResize.reset()
                signposter.endInterval("resize.deliver", interval, "queued=0")
                return
            }
            context.commandSink.sendTerminalResize(
                sessionId: context.sessionId,
                identity: identity,
                claim: claim
            ) { result in
                let queued = result == Int32(KODOSI_FFI_OK)
                context.lastResize.queued(identity, accepted: queued)
                signposter.endInterval(
                    "resize.deliver",
                    interval,
                    "queued=\(queued ? 1 : 0, privacy: .public)"
                )
            }
        }
    }
}
