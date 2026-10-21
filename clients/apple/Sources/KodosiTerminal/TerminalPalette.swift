public struct TerminalPalette: Sendable {
    public let background: String
    public let foreground: String
    public let cursor: String
    public let selectionBackground: String
    public let selectionForeground: String
    public let ansiColors: [String]

    public init(
        background: String,
        foreground: String,
        cursor: String,
        selectionBackground: String,
        selectionForeground: String,
        ansiColors: [String],
    ) {
        self.background = background
        self.foreground = foreground
        self.cursor = cursor
        self.selectionBackground = selectionBackground
        self.selectionForeground = selectionForeground
        self.ansiColors = ansiColors
    }
}

public extension TerminalPalette {
    static let kodosi = TerminalPalette(
        background: "0d0b09",
        foreground: "e8e1d9",
        cursor: "cc7a45",
        selectionBackground: "2b2520",
        selectionForeground: "e8e1d9",
        ansiColors: [
            "1a1613", "c44040", "52b36b", "c4993d",
            "5d8ec7", "b06dba", "5dafa6", "c4b8a8",
            "8c7a6f", "d66060", "6dcc85", "d6b35d",
            "7daad9", "c88dd0", "7dc7be", "e8e1d9",
        ],
    )
}
