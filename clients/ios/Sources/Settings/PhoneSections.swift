import KodosiDesign
import KodosiTerminalSessions
import SwiftUI

struct TerminalSection: View {
    @Environment(AppModel.self) private var model
    @AppStorage(ThemePreference.storageKey) private var themePreference = ThemePreference.system.rawValue
    @AppStorage(TerminalAppearance.TextSize.storageKey) private var textSize = TerminalAppearance.TextSize.standard.rawValue
    @AppStorage(ScreenAwake.storageKey) private var keepScreenOn = false

    private var size: TerminalAppearance.TextSize {
        TerminalAppearance.TextSize(rawValue: textSize) ?? .standard
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            TerminalPreview(family: TerminalAppearance.fontFamily, size: CGFloat(size.points), lineHeight: 1.1, blinks: false)
                .terminalScope()
            ListGroup(footer: "Larger text gives a terminal fewer columns while you type in it here.") {
                ListRow(String(localized: "Appearance"), symbol: "circle.lefthalf.filled", tint: TileTint.graphite) {
                    SegmentedPill(selection: $themePreference, options: [
                        SegmentOption(value: ThemePreference.system.rawValue, title: String(localized: "Auto")),
                        SegmentOption(value: ThemePreference.light.rawValue, title: String(localized: "Light"), symbol: "sun.max"),
                        SegmentOption(value: ThemePreference.dark.rawValue, title: String(localized: "Dark"), symbol: "moon"),
                    ], compact: true, identifier: "settings.appearance")
                }
                ListRow(String(localized: "Size"), symbol: "textformat.size", tint: TileTint.teal) {
                    SegmentedPill(selection: $textSize, options: TerminalAppearance.TextSize.allCases.map {
                        SegmentOption(value: $0.rawValue, title: $0.title)
                    }, compact: true, identifier: "settings.terminal.textSize")
                }
                SwitchRow(String(localized: "Keep the screen on"), subtitle: String(localized: "While a terminal is open"),
                          symbol: "sun.max", tint: TileTint.amber, isOn: $keepScreenOn)
                    .accessibilityIdentifier("settings.terminal.keepScreenOn")
            }
        }
        .sensoryFeedback(.selection, trigger: themePreference)
        .sensoryFeedback(.selection, trigger: textSize)
        .onChange(of: textSize) { _, _ in model.terminals.live.applyStyle(TerminalAppearance.style) }
        .onChange(of: keepScreenOn) { _, _ in ScreenAwake.update() }
    }
}

private extension TerminalAppearance.TextSize {
    var title: String {
        switch self {
        case .smaller: String(localized: "Smaller")
        case .standard: String(localized: "Default")
        case .larger: String(localized: "Larger")
        }
    }
}

struct NotificationsSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL

    var body: some View {
        let denied = model.push.permission == .denied
        ListGroup(title: "Notifications") {
            SwitchRow(String(localized: "Your terminals"),
                      subtitle: denied ? String(localized: "Turn on notifications for Kodosi in Settings")
                          : String(localized: "When one waits for you, finishes or fails"),
                      symbol: "bell.badge", tint: TileTint.orange,
                      isOn: Binding(get: { model.push.enabled && !denied }, set: { on in
                          if on, denied, let url = URL(string: UIApplication.openNotificationSettingsURLString) {
                              openURL(url)
                          } else {
                              model.push.enabled = on
                          }
                      }))
                      .accessibilityIdentifier("settings.notifications")
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            model.push.register()
        }
    }
}

struct PrivacySection: View {
    @Environment(AppModel.self) private var model
    @State private var proof = AppLock.Proof.current

    var body: some View {
        ListGroup(title: "Privacy") {
            SwitchRow(title, subtitle: proof == nil ? String(localized: "Set a passcode on this device first")
                : String(localized: "When Kodosi opens and after a minute away"),
                symbol: symbol, tint: TileTint.green,
                isOn: Binding(get: { model.lock.enabled }, set: { on in Task { await model.lock.turn(on: on) } }))
                .disabled(proof == nil)
                .accessibilityIdentifier("settings.privacy.lock")
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            proof = AppLock.Proof.current
        }
    }

    private var title: String {
        switch proof {
        case .faceID: String(localized: "Lock with Face ID")
        case .touchID: String(localized: "Lock with Touch ID")
        case .opticID: String(localized: "Lock with Optic ID")
        case .passcode, nil: String(localized: "Lock with the passcode")
        }
    }

    private var symbol: String {
        switch proof {
        case .faceID: "faceid"
        case .touchID: "touchid"
        case .opticID: "opticid"
        case .passcode, nil: "lock"
        }
    }
}
