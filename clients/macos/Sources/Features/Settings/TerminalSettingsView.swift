import CoreText
import KodosiDesign
import SwiftUI

struct TerminalSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var draft = DesktopSettings.TerminalSettings.defaults
    @State private var families: [String] = []
    @AppStorage(ThemePreference.storageKey) private var themePreference = ThemePreference.system.rawValue
    @AppStorage(MotionPreference.storageKey) private var reducesMotion = false
    @Environment(\.accessibilityReduceMotion) private var systemReducesMotion

    private var changed: Bool {
        draft.normalized() != deps.settings.terminalSettings
    }

    private func setBranchFolder(_ folder: String?) {
        deps.settings.branchFolder = folder
        deps.settings.save()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            TerminalPreview(family: draft.fontFamily, size: CGFloat(draft.fontSize), lineHeight: draft.lineHeight,
                            cursor: draft.cursorStyle.preview, blinks: draft.cursorBlink)
                .terminalScope()
            ListGroup {
                ListRow(String(localized: "Appearance"), symbol: "circle.lefthalf.filled", tint: TileTint.graphite) {
                    SegmentedPill(selection: $themePreference, options: [
                        SegmentOption(value: ThemePreference.system.rawValue, title: String(localized: "Auto")),
                        SegmentOption(value: ThemePreference.light.rawValue, title: String(localized: "Light"), symbol: "sun.max"),
                        SegmentOption(value: ThemePreference.dark.rawValue, title: String(localized: "Dark"), symbol: "moon"),
                    ], compact: true, identifier: "settings.appearance")
                }
                Toggle(isOn: systemReducesMotion ? .constant(true) : $reducesMotion) {
                    HStack(spacing: 12) {
                        IconTile(symbol: "wind", tint: TileTint.teal, size: 28).bounce(on: reducesMotion)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Reduce motion")
                            if systemReducesMotion {
                                Text("On in System Settings").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            }
                        }
                    }
                }
                .padding(.horizontal, 14).frame(minHeight: 52)
                .disabled(systemReducesMotion)
                .accessibilityIdentifier("settings.reduceMotion")
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
            if changed {
                HStack(spacing: 8) {
                    Spacer()
                    Button("Revert") { draft = deps.settings.terminalSettings }.buttonStyle(.kodosi(.ghost))
                    Button("Apply to all terminals") {
                        draft = deps.settings.commitTerminalSettings(draft)
                        deps.terminalManager.applySettings(draft)
                    }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("settings.terminal.apply")
                }
                .transition(.opacity)
            }
            ListGroup(footer: "A terminal on a new branch gets its own folder. Kodosi removes a folder with no changes when its terminal ends.") {
                ListRow(String(localized: "Branch folders"), subtitle: deps.settings.branchFolder ?? String(localized: "Beside the repository"),
                        monoSubtitle: deps.settings.branchFolder != nil, symbol: "arrow.triangle.branch", tint: TileTint.pink)
                {
                    if deps.settings.branchFolder != nil {
                        Button("Beside the repository") { setBranchFolder(nil) }.buttonStyle(.kodosi(.ghost, size: .small))
                    }
                    Button("Choose…") {
                        Task { @MainActor in
                            if let folder = await pickWorkingDirectory(initialDirectory: deps.settings.branchFolder,
                                                                       message: String(localized: "Choose a folder for new branches"))
                            {
                                setBranchFolder(folder)
                            }
                        }
                    }
                    .buttonStyle(.kodosi(.secondary, size: .small)).accessibilityIdentifier("settings.terminal.branchFolder")
                }
            }
        }
        .animation(theme.motion.snappy, value: changed)
        .onAppear { draft = deps.settings.terminalSettings }
        .task {
            let fixed = await Self.monospacedFamilies()
            families = Array(Set(fixed + [draft.fontFamily, DesktopSettings.TerminalDefaults.fontFamily])).sorted()
        }
    }

    @concurrent
    private static func monospacedFamilies() async -> [String] {
        let monospaced = CTFontDescriptorCreateWithAttributes([
            kCTFontTraitsAttribute: [kCTFontSymbolicTrait: CTFontSymbolicTraits.traitMonoSpace.rawValue],
        ] as CFDictionary)
        let faces = CTFontDescriptorCreateMatchingFontDescriptors(monospaced, nil) as? [CTFontDescriptor] ?? []
        return Array(Set(faces.compactMap { CTFontDescriptorCopyAttribute($0, kCTFontFamilyNameAttribute) as? String }))
    }
}

private extension DesktopSettings.CursorStyle {
    var preview: TerminalPreview.Cursor {
        switch self {
        case .block: .block
        case .bar: .bar
        case .underline: .underline
        }
    }
}
