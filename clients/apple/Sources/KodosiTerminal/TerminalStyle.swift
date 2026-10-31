import CoreGraphics
import GhosttyTerminal

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
        palette: TerminalPalette,
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

    /// The grid of a terminal view of this size in points.
    public func grid(
        fitting size: CGSize,
        scale: Double,
        cellWidthPixels: UInt32,
        cellHeightPixels: UInt32,
    ) -> TerminalViewport {
        let padding = TerminalConfiguration {
            $0.withWindowPaddingX(paddingX)
            $0.withWindowPaddingY(paddingY)
        }
        let grid = padding.grid(
            widthPixels: UInt32((max(0, size.width) * scale).rounded(.down)),
            heightPixels: UInt32((max(0, size.height) * scale).rounded(.down)),
            scale: scale,
            cellWidthPixels: cellWidthPixels,
            cellHeightPixels: cellHeightPixels,
        )
        return TerminalViewport(
            columns: grid.columns,
            rows: grid.rows,
            cellWidthPixels: cellWidthPixels,
            cellHeightPixels: cellHeightPixels,
        )
    }

    var ghosttyFontSize: Float {
        #if os(macOS)
            fontSize
        #else
            fontSize * Self.pointsPerInch / Self.ghosttyDotsPerInch
        #endif
    }

    private static let pointsPerInch: Float = 72
    private static let ghosttyDotsPerInch: Float = 96
}
