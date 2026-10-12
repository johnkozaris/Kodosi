import KodosiTerminal

enum KodosiTerminalStyle {
    static func make(
        settings: DesktopSettings.TerminalSettings,
        fontSize: Float? = nil,
        cursorBlink: Bool? = nil
    ) -> TerminalStyle {
        let cursorStyle: TerminalCursorStyle = switch settings.cursorStyle {
        case .block: .block
        case .bar: .bar
        case .underline: .underline
        }
        return TerminalStyle(
            fontFamily: settings.fontFamily,
            fontSize: fontSize ?? Float(settings.fontSize),
            lineHeightAdjustment: Int(((settings.lineHeight - 1) * 100).rounded()),
            cursorStyle: cursorStyle,
            cursorBlink: cursorBlink ?? settings.cursorBlink,
            scrollback: TerminalScrollbackBudget(lines: settings.scrollbackLines),
            paddingX: 4,
            paddingY: 4,
            minimumContrast: 1.1,
            palette: .kodosi
        )
    }
}
