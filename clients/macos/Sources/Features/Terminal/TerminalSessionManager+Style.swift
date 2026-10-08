import KodosiTerminal

enum KodosiTerminalStyle {
    private static let lightPalette = TerminalPalette(
        background: "e2d8cd",
        foreground: "1c1208",
        cursor: "8c4410",
        selectionBackground: "d1c4b4",
        selectionForeground: "1c1208",
        ansiColors: [
            "1c1208", "a83030", "2a8040", "8c6820",
            "2c6098", "884090", "2a8878", "e2d8cd",
            "403830", "c04848", "409858", "a88038",
            "4478b0", "a058a8", "40a090", "1c1208",
        ]
    )

    private static let darkPalette = TerminalPalette(
        background: "0d0b09",
        foreground: "e8e1d9",
        cursor: "cc7a45",
        selectionBackground: "2b2520",
        selectionForeground: "e8e1d9",
        ansiColors: [
            "1a1613", "c44040", "52b36b", "c4993d",
            "5d8ec7", "b06dba", "5dafa6", "c4b8a8",
            "403830", "d66060", "6dcc85", "d6b35d",
            "7daad9", "c88dd0", "7dc7be", "e8e1d9",
        ]
    )

    static func make(
        settings: DesktopSettings.TerminalSettings,
        fontSize: Float? = nil,
        cursorBlink: Bool? = nil,
        darkOnly: Bool = false
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
            lightPalette: darkOnly ? darkPalette : lightPalette,
            darkPalette: darkPalette
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
