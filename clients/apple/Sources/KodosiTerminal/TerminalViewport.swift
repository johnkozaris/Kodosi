public struct TerminalViewport: Sendable {
    public let columns: UInt16
    public let rows: UInt16
    public let cellWidthPixels: UInt32
    public let cellHeightPixels: UInt32

    public init(
        columns: UInt16,
        rows: UInt16,
        cellWidthPixels: UInt32,
        cellHeightPixels: UInt32
    ) {
        self.columns = columns
        self.rows = rows
        self.cellWidthPixels = cellWidthPixels
        self.cellHeightPixels = cellHeightPixels
    }
}
