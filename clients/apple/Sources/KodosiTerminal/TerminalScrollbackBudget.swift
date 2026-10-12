public struct TerminalScrollbackBudget: Sendable {
    static let bytesPerLine = 2400
    static let allocationPageBytes = 512 * 1024
    static let minimumBytes = 2 * allocationPageBytes
    static let maximumBytes = 256 * 1024 * 1024

    let lines: Int

    public init(lines: Int) {
        self.lines = max(0, lines)
    }

    public var bytes: Int {
        let estimated = lines.multipliedReportingOverflow(by: Self.bytesPerLine)
        let bounded = estimated.overflow
            ? Self.maximumBytes
            : min(max(estimated.partialValue, Self.minimumBytes), Self.maximumBytes)
        let pages = (bounded + Self.allocationPageBytes - 1) / Self.allocationPageBytes
        return min(pages * Self.allocationPageBytes, Self.maximumBytes)
    }
}
