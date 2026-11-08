import KodosiDesign
import KodosiTerminalSessions
import SwiftUI

/// How this device shows terminals: the size of their text, and whether the screen stays on.
struct TerminalSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @AppStorage(TerminalAppearance.TextSize.storageKey) private var textSize = TerminalAppearance.TextSize.standard.rawValue
    @AppStorage(ScreenAwake.storageKey) private var keepScreenOn = false

    var body: some View {
        Section {
            TextSizePicker(selected: $textSize)
                .raisedRows()
            SettingRow(symbol: "sun.max", tint: TileTint.amber, title: Text("Keep the screen on"),
                       subtitle: Text("While a terminal is open"))
            {
                Toggle(isOn: $keepScreenOn) { Text("Keep the screen on") }
                    .labelsHidden()
                    .accessibilityIdentifier("settings.terminal.keepScreenOn")
            }
            .raisedRows()
        } header: {
            GroupedListHeader(title: "Terminal")
        } footer: {
            Text("Larger text gives a terminal fewer columns while you type in it here.")
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
        }
        .onChange(of: textSize) { _, _ in model.terminals.live.applyStyle(TerminalAppearance.style) }
        .onChange(of: keepScreenOn) { _, _ in ScreenAwake.update() }
    }
}

private struct TextSizePicker: View {
    @Environment(\.theme) private var theme
    @Binding var selected: String

    var body: some View {
        HStack(spacing: 12) {
            ForEach(TerminalAppearance.TextSize.allCases, id: \.self) { size in
                let isSelected = size.rawValue == selected
                Button {
                    withAnimation(theme.motion.soft) { selected = size.rawValue }
                } label: {
                    VStack(spacing: 9) {
                        TerminalSwatch(points: CGFloat(size.points))
                            .overlay {
                                if isSelected {
                                    RoundedRectangle(cornerRadius: Radius.md + 4, style: .continuous)
                                        .strokeBorder(theme.colors.accent, lineWidth: 2.5)
                                        .padding(-5)
                                }
                            }
                        HStack(spacing: 5) {
                            Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                                .foregroundStyle(isSelected ? theme.colors.accent : theme.colors.inkFaint)
                                .contentTransition(.symbolEffect(.replace))
                            Text(size.title)
                                .foregroundStyle(isSelected ? theme.colors.ink : theme.colors.inkMuted)
                        }
                        .appTextStyle(.footnote)
                        .fontWeight(.medium)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 6)
                    .contentShape(Rectangle())
                }
                .buttonStyle(PressScaleStyle(scale: 0.96))
                .accessibilityLabel(Text(size.title))
                .accessibilityAddTraits(isSelected ? .isSelected : [])
                .accessibilityIdentifier("settings.terminal.textSize.\(size.rawValue)")
            }
        }
        .padding(.vertical, 10)
        .sensoryFeedback(.selection, trigger: selected)
    }
}

/// A corner of a terminal with its text at a size, in the terminal's colors.
private struct TerminalSwatch: View {
    let points: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: points * 0.3) {
            Text(verbatim: "❯ ").foregroundStyle(Color(red: 0.80, green: 0.48, blue: 0.27)) + Text(verbatim: "git status")
            Text(verbatim: "On branch main")
        }
        .font(.system(size: points, design: .monospaced))
        .foregroundStyle(Color(red: 0.91, green: 0.88, blue: 0.85))
        .lineLimit(1)
        .fixedSize()
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, minHeight: 62, alignment: .leading)
        .clipped()
        .background(Color(red: 0.05, green: 0.04, blue: 0.035), in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
        .accessibilityHidden(true)
    }
}

private extension TerminalAppearance.TextSize {
    var title: LocalizedStringKey {
        switch self {
        case .smaller: "Smaller"
        case .standard: "Default"
        case .larger: "Larger"
        }
    }
}

/// Kodosi's lock on this device, with the proof that this device has.
struct PrivacySection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var proof = AppLock.Proof.current

    var body: some View {
        Section {
            SettingRow(symbol: symbol, tint: TileTint.green, title: title,
                       subtitle: proof == nil ? Text("Set a passcode on this device first")
                           : Text("When Kodosi opens and after a minute away"))
            {
                Toggle(isOn: Binding(get: { model.lock.enabled }, set: { on in Task { await model.lock.turn(on: on) } })) {
                    title
                }
                .labelsHidden()
                .disabled(proof == nil)
                .accessibilityIdentifier("settings.privacy.lock")
            }
            .raisedRows()
        } header: {
            GroupedListHeader(title: "Privacy")
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            proof = AppLock.Proof.current
        }
    }

    private var title: Text {
        switch proof {
        case .faceID: Text("Lock with Face ID")
        case .touchID: Text("Lock with Touch ID")
        case .opticID: Text("Lock with Optic ID")
        case .passcode, nil: Text("Lock with the passcode")
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
