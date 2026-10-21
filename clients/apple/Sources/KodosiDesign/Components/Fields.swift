import SwiftUI

public struct WellTextFieldStyle: @preconcurrency TextFieldStyle {
    @Environment(\.theme) private var theme
    @FocusState private var focused: Bool
    public var radius: CGFloat = Radius.md
    public var compact = false

    public init(radius: CGFloat = Radius.md, compact: Bool = false) {
        self.radius = radius
        self.compact = compact
    }

    @MainActor
    public func _body(configuration: TextField<Self._Label>) -> some View {
        configuration
            .textFieldStyle(.plain)
            .appTextStyle(compact ? .footnote : .body)
            .focused($focused)
            .padding(.horizontal, compact ? 10 : 12)
            .frame(minHeight: compact ? KodosiButtonStyle.Size.small.height : 34)
            .well(radius)
            .overlay {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(theme.colors.accent.opacity(focused ? 0.55 : 0), lineWidth: 1.5)
            }
            .animation(theme.motion.hover, value: focused)
    }
}

public struct KodosiSwitchStyle: ToggleStyle {
    @Environment(\.theme) private var theme

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 12) {
                configuration.label.appTextStyle(.body).foregroundStyle(theme.colors.ink)
                Spacer(minLength: 12)
                Capsule()
                    .fill((configuration.isOn ? theme.colors.accent : theme.colors.well).shadow(.inner(
                        color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.12), radius: 2, y: 1,
                    )))
                    .frame(width: 36, height: 21)
                    .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                        Circle().fill(.white)
                            .shadow(color: .black.opacity(0.25), radius: 1.5, y: 1)
                            .frame(width: 17, height: 17).padding(2)
                    }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .animation(theme.motion.snappy, value: configuration.isOn)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
        .accessibilityValue(Text(configuration.isOn ? "On" : "Off"))
        .accessibilityAddTraits(configuration.isOn ? [.isSelected] : [])
    }
}

public struct KodosiStepper: View {
    @Environment(\.theme) private var theme
    public let title: LocalizedStringKey
    @Binding public var value: Int
    public let range: ClosedRange<Int>
    public var step = 1

    public init(title: LocalizedStringKey, value: Binding<Int>, range: ClosedRange<Int>, step: Int = 1) {
        self.title = title
        _value = value
        self.range = range
        self.step = step
    }

    public var body: some View {
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

public struct MenuRow: View {
    @Environment(\.theme) private var theme
    public let title: String
    public var symbol: String?
    public var detail: String?
    public var selected = false
    public var destructive = false

    public init(title: String, symbol: String? = nil, detail: String? = nil, selected: Bool = false, destructive: Bool = false) {
        self.title = title
        self.symbol = symbol
        self.detail = detail
        self.selected = selected
        self.destructive = destructive
    }

    public var body: some View {
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

public struct MenuPicker<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    @State private var expanded = false
    public let title: LocalizedStringKey
    @Binding public var selection: Value
    public let values: [Value]
    public let label: (Value) -> String
    public var identifier = "picker"

    public init(
        title: LocalizedStringKey,
        selection: Binding<Value>,
        values: [Value],
        label: @escaping (Value) -> String, identifier: String = "picker",
    ) {
        self.title = title
        _selection = selection
        self.values = values
        self.label = label
        self.identifier = identifier
    }

    public var body: some View {
        Button { expanded.toggle() } label: {
            HStack(spacing: 8) {
                Text(label(selection)).lineLimit(1)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9, weight: .bold)).foregroundStyle(theme.colors.inkFaint)
            }
        }
        .buttonStyle(.kodosi(.secondary))
        .accessibilityLabel(Text(title)).accessibilityValue(label(selection)).accessibilityIdentifier(identifier)
        .kodosiPopover(isPresented: $expanded, arrowEdge: .bottom) {
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
