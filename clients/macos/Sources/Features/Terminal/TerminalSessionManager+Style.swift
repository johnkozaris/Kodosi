import KodosiTerminal

enum KodosiTerminalStyle {
    private static let palette = TerminalPalette(
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
        ]
    )

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
            palette: palette
        )
    }
}

extension TerminalSessionManager {
    func kodosiTerminalStyle(
        settings terminalSettings: DesktopSettings.TerminalSettings? = nil,
        fontSize: Float? = nil
    ) -> TerminalStyle {
        KodosiTerminalStyle.make(
            settings: terminalSettings ?? settings.terminalSettings,
            fontSize: fontSize
        )
    }
}
