import AppKit
import KodosiBridge
import KodosiTerminal
import KodosiTerminalSessions

extension AppDependencies {
    /// The grid of the tile that a new terminal takes, so that its program starts at its final width.
    func startSize(computer: String?, roomId: String?) -> JSONValue? {
        guard computer == nil, roomId == nil, let room = workbench.tileRoom,
              let scale = MainWindowLocator.mainWindow()?.backingScaleFactor,
              let cell = terminalCell(scale: scale),
              let grid = Self.startGrid(
                  room: room, tiles: workbench.focusedSessionId == nil ? workbench.stagedSessionIds.count + 1 : 1,
                  scale: scale, cell: cell, style: KodosiTerminalStyle.make(settings: settings.terminalSettings),
              )
        else { return nil }
        return .object([
            "cols": .int(Int64(grid.columns)), "rows": .int(Int64(grid.rows)),
            "cellWidthPixels": .int(Int64(grid.cellWidthPixels)), "cellHeightPixels": .int(Int64(grid.cellHeightPixels)),
        ])
    }

    static func startGrid(
        room: CGSize, tiles: Int, scale: CGFloat, cell: DesktopSettings.TerminalCell, style: TerminalStyle,
    ) -> TerminalViewport? {
        TerminalTilesLayout.frames(count: tiles, in: CGRect(origin: .zero, size: room)).last.map { tile in
            style.grid(
                fitting: SessionTileView.terminalSize(in: tile.size), scale: scale,
                cellWidthPixels: cell.width, cellHeightPixels: cell.height,
            )
        }
    }

    /// The first terminal after a launch has no other on screen, so the last cell size is saved.
    private func terminalCell(scale: CGFloat) -> DesktopSettings.TerminalCell? {
        if let live = terminalManager.viewportOnScreen {
            let cell = DesktopSettings.TerminalCell(width: live.cellWidthPixels, height: live.cellHeightPixels, scale: scale)
            if settings.terminalCell != cell {
                settings.terminalCell = cell
            }
        }
        return settings.terminalCell.flatMap { $0.scale == scale ? $0 : nil }
    }
}
