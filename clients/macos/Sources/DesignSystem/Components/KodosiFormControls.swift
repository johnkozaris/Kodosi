import SwiftUI

struct KodosiTextFieldStyle: TextFieldStyle {
    @Environment(\.theme) private var theme

    func _body(configuration: TextField<Self._Label>) -> some View {
        configuration.textFieldStyle(.plain).font(.system(size: 13))
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(theme.colors.surfaceStage, in: RoundedRectangle(cornerRadius: theme.radius.sm))
            .overlay(alignment: .bottom) { Rectangle().fill(theme.colors.seam).frame(height: 1) }
    }
}

struct KodosiPicker<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    @State private var expanded = false
    let title: LocalizedStringKey
    @Binding var selection: Value
    let values: [Value]
    let label: (Value) -> String

    init(_ title: LocalizedStringKey, selection: Binding<Value>, values: [Value], label: @escaping (Value) -> String) {
        self.title = title
        _selection = selection
        self.values = values
        self.label = label
    }

    var body: some View {
        HStack {
            Text(title).appTextStyle(.body)
            Spacer(minLength: 12)
            Button { expanded.toggle() } label: {
                HStack(spacing: 16) {
                    Text(label(selection))
                    Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold))
                }
            }
            .buttonStyle(SolidSecondaryButtonStyle()).accessibilityLabel(Text(title))
            .accessibilityValue(label(selection))
            .popover(isPresented: $expanded, arrowEdge: .bottom) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(title).appTextStyle(.headingItem).foregroundStyle(theme.colors.mutedForeground)
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                            Button { selection = value; expanded = false } label: {
                                PopoverActionRow(icon: value == selection ? "checkmark" : "",
                                                 title: label(value))
                                    .background(RoundedRectangle(cornerRadius: theme.radius.sm)
                                        .fill(value == selection ? theme.colors.primary.opacity(0.07) : theme.colors.card.opacity(0.6)))
                                    .overlay(RoundedRectangle(cornerRadius: theme.radius.sm)
                                        .stroke(value == selection ? theme.colors.primary.opacity(0.55) : theme.colors.border, lineWidth: 1))
                            }.buttonStyle(.plain)
                        }
                    }
                }.padding(16).frame(minWidth: 220).popoverSurface()
            }
        }
    }
}

struct KodosiToggleStyle: ToggleStyle {
    @Environment(\.theme) private var theme

    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 10) {
                Image(systemName: "checkmark").opacity(configuration.isOn ? 1 : 0)
                    .font(.system(size: 10, weight: .bold)).frame(width: 18, height: 18)
                    .foregroundStyle(configuration.isOn ? theme.colors.primaryForeground : theme.colors.foreground)
                    .background(ElevatedSurface(fill: configuration.isOn ? theme.colors.primary : theme.colors.secondary, isPressed: false))
                configuration.label.appTextStyle(.body)
            }
        }
        .buttonStyle(.plain)
        .accessibilityValue(Text(configuration.isOn ? "On" : "Off"))
        .accessibilityAddTraits(configuration.isOn ? [.isSelected] : [])
    }
}

struct KodosiActionMenu<Content: View>: View {
    @Environment(\.theme) private var theme
    @State private var expanded = false
    let title: LocalizedStringKey
    @ViewBuilder let content: () -> Content

    init(_ title: LocalizedStringKey, @ViewBuilder content: @escaping () -> Content) {
        self.title = title
        self.content = content
    }

    var body: some View {
        Button { expanded.toggle() } label: { HStack { Text(title); Image(systemName: "chevron.down") } }
            .buttonStyle(SolidSecondaryButtonStyle())
            .popover(isPresented: $expanded, arrowEdge: .bottom) {
                VStack(alignment: .leading, spacing: 12) {
                    Text(title).appTextStyle(.headingItem).foregroundStyle(theme.colors.mutedForeground)
                    VStack(alignment: .leading, spacing: 6, content: content)
                        .buttonStyle(PopoverMenuButtonStyle())
                }
                .padding(16).frame(minWidth: 240).popoverSurface()
                .simultaneousGesture(TapGesture().onEnded { expanded = false })
            }
    }
}

struct KodosiStepper: View {
    let title: LocalizedStringKey
    @Binding var value: Int
    let range: ClosedRange<Int>
    var step = 1

    var body: some View {
        HStack {
            Text(title).appTextStyle(.body)
            Spacer()
            Button { value = max(range.lowerBound, value - step) } label: { Image(systemName: "minus") }
                .disabled(value <= range.lowerBound).accessibilityLabel(Text("Decrease"))
            Text(value.formatted()).monospacedDigit().frame(minWidth: 50)
            Button { value = min(range.upperBound, value + step) } label: { Image(systemName: "plus") }
                .disabled(value >= range.upperBound).accessibilityLabel(Text("Increase"))
        }.buttonStyle(SolidSecondaryButtonStyle())
    }
}
