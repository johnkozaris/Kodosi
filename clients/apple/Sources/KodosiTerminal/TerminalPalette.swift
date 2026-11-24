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
    static let dark = TerminalPalette(
        background: "0d0b09",
        foreground: "e8e1d9",
        cursor: "cc7a45",
        selectionBackground: "5c3825",
        selectionForeground: "e8e1d9",
        ansiColors: [
            "1a1613", "c44040", "52b36b", "c4993d",
            "5d8ec7", "b06dba", "5dafa6", "c4b8a8",
            "8c7a6f", "d66060", "6dcc85", "d6b35d",
            "7daad9", "c88dd0", "7dc7be", "e8e1d9",
        ],
    )

    /// White and bright white are mid greys, so that a program's white text stays readable.
    static let light = TerminalPalette(
        background: "faf6ee",
        foreground: "2a1d15",
        cursor: "b85a26",
        selectionBackground: "f0dfcb",
        selectionForeground: "2a1d15",
        ansiColors: [
            "2a1d15", "b23a32", "2f7d54", "7e5e12",
            "2e64a8", "8f3f9c", "1f7a74", "7d6b5f",
            "5e4d42", "c4453b", "3a8f60", "94701a",
            "3a74be", "a24db0", "2a8f88", "9a8778",
        ],
    )
}
