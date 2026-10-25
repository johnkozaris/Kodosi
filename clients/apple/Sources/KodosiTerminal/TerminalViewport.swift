public struct TerminalViewport: Sendable {
    public let columns: UInt16
    public let rows: UInt16
    public let cellWidthPixels: UInt32
    public let cellHeightPixels: UInt32

    public init(
        columns: UInt16,
        rows: UInt16,
        cellWidthPixels: UInt32,
        cellHeightPixels: UInt32,
    ) {
        self.columns = columns
        self.rows = rows
        self.cellWidthPixels = cellWidthPixels
        self.cellHeightPixels = cellHeightPixels
    }
}

public struct TerminalRowMetrics: Equatable, Sendable {
    public let cellHeightPixels: UInt32
    public let insetPixels: UInt32

    public init(rows: UInt16, heightPixels: UInt32, cellHeightPixels: UInt32) {
        self.cellHeightPixels = cellHeightPixels
        insetPixels = heightPixels - min(heightPixels, UInt32(rows) * cellHeightPixels)
    }
}
