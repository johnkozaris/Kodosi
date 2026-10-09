import SwiftUI

struct WellTextFieldStyle: @preconcurrency TextFieldStyle {
    @Environment(\.theme) private var theme
    @FocusState private var focused: Bool
    var radius: CGFloat = Radius.md

    @MainActor
    func _body(configuration: TextField<Self._Label>) -> some View {
        configuration
            .textFieldStyle(.plain)
            .appTextStyle(.body)
            .focused($focused)
            .padding(.horizontal, 12)
            .frame(minHeight: 34)
            .well(radius)
            .overlay {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(theme.colors.accent.opacity(focused ? 0.55 : 0), lineWidth: 1.5)
            }
            .animation(theme.motion.hover, value: focused)
    }
}

struct SwitchToggleStyle: ToggleStyle {
    @Environment(\.theme) private var theme

    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 12) {
                configuration.label.appTextStyle(.body).foregroundStyle(theme.colors.ink)
                Spacer(minLength: 12)
                Capsule()
                    .fill((configuration.isOn ? theme.colors.accent : theme.colors.well).shadow(.inner(
                        color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.12), radius: 2, y: 1
                    )))
                    .frame(width: 36, height: 21)
                    .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                        Circle().fill(.white)
                            .shadow(color: .black.opacity(0.25), radius: 1.5, y: 1)
                            .frame(width: 17, height: 17).padding(2)
                    }
            }
            .contentShape(Rectangle())
            .animation(theme.motion.snappy, value: configuration.isOn)
        }
        .buttonStyle(.plain)
        .accessibilityValue(Text(configuration.isOn ? "On" : "Off"))
        .accessibilityAddTraits(configuration.isOn ? [.isSelected] : [])
    }
}

struct KodosiStepper: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    @Binding var value: Int
    let range: ClosedRange<Int>
    var step = 1

    var body: some View {
        HStack(spacing: 2) {
            IconButton(title: "Decrease", symbol: "minus", identifier: "stepper.decrease", size: 26) {
                value = max(range.lowerBound, value - step)
            }.disabled(value <= range.lowerBound)
            Text(value.formatted()).appTextStyle(.subhead).monospacedDigit()
                .contentTransition(.numericText(value: Double(value)))
                .foregroundStyle(theme.colors.ink)
                .frame(minWidth: 52)
            IconButton(title: "Increase", symbol: "plus", identifier: "stepper.increase", size: 26) {
                value = min(range.upperBound, value + step)
            }.disabled(value >= range.upperBound)
        }
        .padding(3)
        .wellCapsule()
        .animation(theme.motion.snappy, value: value)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(title))
    }
}

struct MenuRow: View {
    @Environment(\.theme) private var theme
    let title: String
    var symbol: String?
    var detail: String?
    var selected = false
    var destructive = false

    var body: some View {
        HStack(spacing: 10) {
            if let symbol {
                Image(systemName: symbol).font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(destructive ? theme.colors.danger : theme.colors.inkMuted).frame(width: 18)
            }
            Text(title).appTextStyle(.body).foregroundStyle(destructive ? theme.colors.danger : theme.colors.ink).lineLimit(1)
            Spacer(minLength: 8)
            if let detail {
                Text(detail).appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
            }
            if selected {
                Image(systemName: "checkmark").font(.system(size: 11, weight: .bold)).foregroundStyle(theme.colors.accentStrong)
                    .transition(AnyTransition.pop)
            }
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 32)
        .hoverRow(radius: Radius.sm)
    }
}

struct MenuPicker<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    @State private var expanded = false
    let title: LocalizedStringKey
    @Binding var selection: Value
    let values: [Value]
    let label: (Value) -> String
    var identifier = "picker"

    var body: some View {
        Button { expanded.toggle() } label: {
            HStack(spacing: 8) {
                Text(label(selection)).lineLimit(1)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .buttonStyle(.kodosi(.secondary))
        .accessibilityLabel(Text(title)).accessibilityValue(label(selection)).accessibilityIdentifier(identifier)
        .popover(isPresented: $expanded, arrowEdge: .bottom) {
            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                        Button { selection = value; expanded = false } label: {
                            MenuRow(title: label(value), selected: value == selection)
                        }.buttonStyle(.plain)
                    }
                }.padding(6)
            }
            .frame(minWidth: 200, maxHeight: 320)
            .popoverSheet()
        }
    }
}
