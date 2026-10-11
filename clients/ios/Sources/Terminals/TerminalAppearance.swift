import KodosiTerminal
import UIKit

enum TerminalAppearance {
    static var style: TerminalStyle {
        TerminalStyle(
            fontFamily: "JetBrains Mono",
            fontSize: UIDevice.current.userInterfaceIdiom == .pad ? 14 : 12,
            lineHeightAdjustment: 10,
            cursorStyle: .block,
            cursorBlink: false,
            scrollback: TerminalScrollbackBudget(lines: 5000),
            paddingX: 4,
            paddingY: 4,
            minimumContrast: 1.1,
            palette: .kodosi
        )
    }
}
