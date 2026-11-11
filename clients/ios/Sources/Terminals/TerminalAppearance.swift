import KodosiTerminal
import UIKit

enum TerminalAppearance {
    /// A larger size gives the terminal fewer columns while this device types in it.
    enum TextSize: String, CaseIterable {
        case smaller
        case standard
        case larger

        static let storageKey = "terminal.textSize"

        var points: Float {
            let usual: Float = UIDevice.current.userInterfaceIdiom == .pad ? 14 : 12
            return switch self {
            case .smaller: usual - 2
            case .standard: usual
            case .larger: usual + 2
            }
        }
    }

    static let fontFamily = "JetBrains Mono"

    static var textSize: TextSize {
        TextSize(rawValue: UserDefaults.standard.string(forKey: TextSize.storageKey) ?? "") ?? .standard
    }

    static var style: TerminalStyle {
        TerminalStyle(
            fontFamily: fontFamily,
            fontSize: textSize.points,
            lineHeightAdjustment: 10,
            cursorStyle: .block,
            cursorBlink: false,
            scrollback: TerminalScrollbackBudget(lines: 5000),
            paddingX: 4,
            paddingY: 4,
            minimumContrast: 1.1,
            palette: .kodosi,
        )
    }
}

enum ScreenAwake {
    static let storageKey = "terminal.keepScreenOn"
    private static var terminals = 0

    static func terminalShown() {
        terminals += 1
        update()
    }

    static func terminalGone() {
        terminals = max(0, terminals - 1)
        update()
    }

    static func update() {
        UIApplication.shared.isIdleTimerDisabled = terminals > 0 && UserDefaults.standard.bool(forKey: storageKey)
    }
}
