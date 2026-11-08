import KodosiTerminal
import UIKit

enum TerminalAppearance {
    /// The size of terminal text that a person picks on this device. A larger size gives the
    /// terminal fewer columns while this device types in it.
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

    static var textSize: TextSize {
        TextSize(rawValue: UserDefaults.standard.string(forKey: TextSize.storageKey) ?? "") ?? .standard
    }

    static var style: TerminalStyle {
        TerminalStyle(
            fontFamily: "JetBrains Mono",
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

/// Keeps the screen of this device on while a terminal shows, when the person chose it.
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
