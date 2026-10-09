public struct TerminalStyle: Sendable {
    public let fontFamily: String
    public let fontSize: Float
    public let lineHeightAdjustment: Int
    public let cursorStyle: TerminalCursorStyle
    public let cursorBlink: Bool
    public let scrollback: TerminalScrollbackBudget
    public let paddingX: Int
    public let paddingY: Int
    public let minimumContrast: Double
    public let palette: TerminalPalette

    public init(
        fontFamily: String,
        fontSize: Float,
        lineHeightAdjustment: Int,
        cursorStyle: TerminalCursorStyle,
        cursorBlink: Bool,
        scrollback: TerminalScrollbackBudget,
        paddingX: Int,
        paddingY: Int,
        minimumContrast: Double,
        palette: TerminalPalette
    ) {
        self.fontFamily = fontFamily
        self.fontSize = fontSize
        self.lineHeightAdjustment = lineHeightAdjustment
        self.cursorStyle = cursorStyle
        self.cursorBlink = cursorBlink
        self.scrollback = scrollback
        self.paddingX = paddingX
        self.paddingY = paddingY
        self.minimumContrast = minimumContrast
        self.palette = palette
    }
}
