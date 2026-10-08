import SwiftUI

struct TerminalSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var draft = DesktopSettings.TerminalSettings.defaults
    @AppStorage(ThemePreference.storageKey) private var themePreference = ThemePreference.system.rawValue

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Terminal").appTextStyle(.headingSection)
            KodosiPicker("Appearance", selection: $themePreference, values: ["system", "light", "dark"], label: {
                switch $0 {
                case "light": String(localized: "Light")
                case "dark": String(localized: "Dark")
                default: String(localized: "System")
                }
            })
            TextField("Font family", text: $draft.fontFamily).textFieldStyle(KodosiTextFieldStyle())
            KodosiStepper(title: "Font size", value: $draft.fontSize, range: DesktopSettings.TerminalDefaults.fontSizeRange)
            KodosiPicker("Cursor", selection: $draft.cursorStyle, values: DesktopSettings.CursorStyle.allCases, label: { $0.label })
            Toggle("Blink cursor", isOn: $draft.cursorBlink)
            HStack {
                Text("Line height")
                Slider(value: $draft.lineHeight, in: DesktopSettings.TerminalDefaults.lineHeightRange, step: 0.05)
                Text(draft.lineHeight, format: .number.precision(.fractionLength(2))).monospacedDigit()
            }
            KodosiStepper(title: "Scrollback lines", value: $draft.scrollbackLines,
                          range: DesktopSettings.TerminalDefaults.scrollbackRange, step: 1000)
            Button("Apply") {
                draft = deps.settings.commitTerminalSettings(draft)
                deps.terminalManager.applySettings(draft)
            }.buttonStyle(SolidPrimaryButtonStyle()).accessibilityIdentifier("settings.terminal.apply")
        }
        .tint(theme.colors.primary)
        .onAppear { draft = deps.settings.terminalSettings }
    }
}
