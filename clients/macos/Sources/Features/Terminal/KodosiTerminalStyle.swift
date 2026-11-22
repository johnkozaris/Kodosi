import KodosiBridge
import KodosiTerminal
import KodosiTerminalSessions

enum KodosiTerminalStyle {
    static func make(settings: DesktopSettings.TerminalSettings, look: TerminalLook) -> TerminalStyle {
        let cursorStyle: TerminalCursorStyle = switch settings.cursorStyle {
        case .block: .block
        case .bar: .bar
        case .underline: .underline
        }
        return TerminalStyle(
            fontFamily: settings.fontFamily,
            fontSize: Float(settings.fontSize),
            lineHeightAdjustment: Int(((settings.lineHeight - 1) * 100).rounded()),
            cursorStyle: cursorStyle,
            cursorBlink: settings.cursorBlink,
            scrollback: TerminalScrollbackBudget(lines: settings.scrollbackLines),
            paddingX: 4,
            paddingY: 4,
            minimumContrast: look.minimumContrast,
            palette: look.palette,
        )
    }
}
