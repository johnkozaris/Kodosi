import AppKit
import SwiftUI

struct TerminalSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var draft = DesktopSettings.TerminalSettings.defaults
    @State private var families: [String] = []
    @AppStorage(ThemePreference.storageKey) private var themePreference = ThemePreference.system.rawValue

    private var changed: Bool {
        draft.normalized() != deps.settings.terminalSettings
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            TerminalPreview(settings: draft).terminalScope()
            ListGroup {
                ListRow(String(localized: "Appearance"), symbol: "circle.lefthalf.filled", tint: TileTint.graphite) {
                    SegmentedPill(selection: $themePreference, options: [
                        SegmentOption(value: "system", title: String(localized: "Auto")),
                        SegmentOption(value: "light", title: String(localized: "Light"), symbol: "sun.max"),
                        SegmentOption(value: "dark", title: String(localized: "Dark"), symbol: "moon"),
                    ], compact: true, identifier: "settings.appearance")
                }
                ListRow(String(localized: "Font"), symbol: "textformat", tint: TileTint.blue) {
                    MenuPicker(title: "Font", selection: $draft.fontFamily, values: families, label: { $0 }, identifier: "settings.terminal.font")
                }
                ListRow(String(localized: "Size"), symbol: "textformat.size", tint: TileTint.teal) {
                    KodosiStepper(title: "Font size", value: $draft.fontSize, range: DesktopSettings.TerminalDefaults.fontSizeRange)
                }
                ListRow(String(localized: "Line height"), symbol: "arrow.up.and.down.text.horizontal", tint: TileTint.purple) {
                    Slider(value: $draft.lineHeight, in: DesktopSettings.TerminalDefaults.lineHeightRange, step: 0.05)
                        .frame(width: 170).accessibilityLabel(Text("Line height"))
                    Text(draft.lineHeight, format: .number.precision(.fractionLength(2))).appTextStyle(.subhead).monospacedDigit()
                        .foregroundStyle(theme.colors.inkMuted).frame(width: 36, alignment: .trailing)
                }
            }
            ListGroup {
                ListRow(String(localized: "Cursor"), symbol: "character.cursor.ibeam", tint: TileTint.orange) {
                    SegmentedPill(selection: $draft.cursorStyle, options: DesktopSettings.CursorStyle.allCases.map {
                        SegmentOption(value: $0, title: $0.label)
                    }, compact: true, identifier: "settings.terminal.cursor")
                }
                Toggle(isOn: $draft.cursorBlink) {
                    HStack(spacing: 12) {
                        IconTile(symbol: "sparkle", tint: TileTint.amber, size: 28)
                        Text("Blink the cursor")
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 52)
                ListRow(String(localized: "Scrollback"), subtitle: String(localized: "Lines kept for each terminal"),
                        symbol: "arrow.up.to.line", tint: TileTint.green)
                {
                    KodosiStepper(title: "Scrollback lines", value: $draft.scrollbackLines,
                                  range: DesktopSettings.TerminalDefaults.scrollbackRange, step: 1000)
                }
            }
            HStack(spacing: 8) {
                Spacer()
                if changed {
                    Button("Revert") { draft = deps.settings.terminalSettings }.buttonStyle(.kodosi(.ghost)).transition(.opacity)
                }
                Button(changed ? "Apply to all terminals" : "Applied") {
                    draft = deps.settings.commitTerminalSettings(draft)
                    deps.terminalManager.applySettings(draft)
                }
                .buttonStyle(.kodosi(changed ? .primary : .ghost)).disabled(!changed)
                .accessibilityIdentifier("settings.terminal.apply")
            }
        }
        .animation(theme.motion.snappy, value: changed)
        .onAppear {
            draft = deps.settings.terminalSettings
            let fixed = NSFontManager.shared.availableFontFamilies.filter { family in
                NSFont(name: family, size: 12)?.isFixedPitch == true
                    || NSFontManager.shared.font(withFamily: family, traits: [], weight: 5, size: 12)?.isFixedPitch == true
            }
            families = Array(Set(fixed + [draft.fontFamily, DesktopSettings.TerminalDefaults.fontFamily])).sorted()
        }
    }
}

private struct TerminalPreview: View {
    @Environment(\.theme) private var theme
    let settings: DesktopSettings.TerminalSettings

    private var font: Font {
        let size = CGFloat(settings.fontSize)
        return NSFont(name: settings.fontFamily, size: size) == nil
            && NSFontManager.shared.font(withFamily: settings.fontFamily, traits: [], weight: 5, size: size) == nil
            ? .system(size: size, design: .monospaced) : .custom(settings.fontFamily, size: size)
    }

    var body: some View {
        let size = CGFloat(settings.fontSize)
        let ink = hex(0xE8E1D9)
        VStack(alignment: .leading, spacing: max(0, (settings.lineHeight - 1) * size)) {
            HStack(spacing: size * 0.5) {
                Text(verbatim: "~/kodosi").foregroundStyle(hex(0x5DAFA6))
                Text(verbatim: "❯").foregroundStyle(theme.colors.accent)
                Text(verbatim: "claude").foregroundStyle(ink)
            }
            Text(verbatim: "✳ Welcome back. What are we building?").foregroundStyle(hex(0xD97757))
            HStack(spacing: size * 0.5) {
                Text(verbatim: "~/kodosi").foregroundStyle(hex(0x5DAFA6))
                Text(verbatim: "❯").foregroundStyle(theme.colors.accent)
                cursor(size: size)
            }
        }
        .font(font)
        .lineLimit(1)
        .padding(18)
        .frame(maxWidth: .infinity, minHeight: 128, alignment: .topLeading)
        .background(theme.colors.terminal, in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).strokeBorder(theme.colors.hairline.opacity(0.7), lineWidth: 0.5) }
        .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.14), radius: 12, x: 3, y: 8)
        .animation(theme.motion.spring, value: settings)
        .accessibilityElement(children: .ignore).accessibilityLabel(Text("Terminal preview"))
    }

    @ViewBuilder
    private func cursor(size: CGFloat) -> some View {
        switch settings.cursorStyle {
        case .block: CursorBlock(width: size * 0.6, height: size * 1.15, blinks: settings.cursorBlink).id(settings.cursorBlink)
        case .bar: CursorBlock(width: 2, height: size * 1.15, blinks: settings.cursorBlink).id(settings.cursorBlink)
        case .underline:
            CursorBlock(width: size * 0.6, height: 2, blinks: settings.cursorBlink).id(settings.cursorBlink)
                .frame(height: size * 1.15, alignment: .bottom)
        }
    }
}
