import KodosiBridge
import KodosiDesign
import SwiftUI

struct FirstPrompt: View {
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var typeSize
    let agent: AgentInfo
    @Binding var prompt: String
    var typing: FocusState<Bool>.Binding

    var body: some View {
        let layout = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6))
            : AnyLayout(HStackLayout(alignment: .top, spacing: 8))
        layout {
            HStack(alignment: .top, spacing: 8) {
                Text(verbatim: "❯").foregroundStyle(theme.colors.accent).accessibilityHidden(true)
                Text(verbatim: agent.program).foregroundStyle(agent.kind.tint.mix(with: .white, by: 0.35))
            }
            .fixedSize()
            TextField("First prompt", text: $prompt, prompt: Text("Type a task").foregroundStyle(theme.colors.inkFaint), axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1 ... 6)
                .focused(typing)
                .submitLabel(.done)
                .foregroundStyle(theme.colors.ink)
                .tint(theme.colors.accent)
                .accessibilityLabel(Text("First prompt"))
                .accessibilityIdentifier("terminals.new.prompt")
        }
        .appTextStyle(.mono)
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture { typing.wrappedValue = true }
        .terminalTile(radius: Radius.lg)
        .onChange(of: prompt) { _, words in
            guard words.contains(where: \.isNewline) else { return }
            prompt = String(words.map { $0.isNewline ? " " : $0 }).trimmingCharacters(in: .whitespaces)
            typing.wrappedValue = false
        }
    }
}

struct ProgramChoice: View {
    @Environment(\.theme) private var theme
    let id: String
    let kind: AgentKind
    let selected: Bool
    let choose: () -> Void

    var body: some View {
        Button(action: choose) {
            HStack(spacing: 8) {
                AgentMark(kind: kind, size: 22)
                Text(kind.label).appTextStyle(.subhead).foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
                if selected {
                    Image(systemName: "checkmark").font(.system(size: 10, weight: .bold))
                        .foregroundStyle(theme.colors.accentStrong).transition(AnyTransition.pop)
                }
            }
            .padding(.leading, 8).padding(.trailing, 13).frame(minHeight: 38)
            .background {
                if selected {
                    RaisedBackground(shape: Capsule(), fill: theme.colors.lifted, elevation: .resting)
                } else {
                    WellBackground(shape: Capsule())
                }
            }
            .touchTarget()
        }
        .buttonStyle(PressScaleStyle())
        .animation(theme.motion.snappy, value: selected)
        .accessibilityLabel(Text(kind.label))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("terminals.new.agent.\(id)")
    }
}

struct SheetRow<Leading: View, Trailing: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var typeSize
    let title: String
    var subtitle: String?
    var monoSubtitle = false
    var titleColor: Color?
    @ViewBuilder let leading: () -> Leading
    @ViewBuilder let trailing: () -> Trailing

    var body: some View {
        let layout = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
            : AnyLayout(HStackLayout(spacing: 12))
        HStack(alignment: typeSize.isAccessibilitySize ? .top : .center, spacing: 12) {
            leading().frame(width: 28)
            layout {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).appTextStyle(.body).foregroundStyle(titleColor ?? theme.colors.ink)
                    if let subtitle {
                        Text(subtitle).appTextStyle(monoSubtitle ? .monoCaption : .footnote).foregroundStyle(theme.colors.inkMuted)
                            .lineLimit(typeSize.isAccessibilitySize ? nil : 2).truncationMode(.middle)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                trailing().layoutPriority(1)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
        .frame(maxWidth: .infinity, minHeight: 52, alignment: .leading)
        .contentShape(Rectangle())
    }
}

extension SheetRow where Trailing == EmptyView {
    init(title: String, subtitle: String? = nil, monoSubtitle: Bool = false, titleColor: Color? = nil, @ViewBuilder leading: @escaping () -> Leading) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle, titleColor: titleColor, leading: leading) { EmptyView() }
    }
}

struct ChoiceRow<Value: Hashable, Leading: View, Accessory: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var typeSize
    let title: String
    var subtitle: String?
    let value: String
    var loading = false
    let values: [Value]
    let selection: Value
    let label: (Value) -> String
    let choose: (Value) -> Void
    @ViewBuilder let leading: () -> Leading
    @ViewBuilder let accessory: () -> Accessory

    var body: some View {
        if values.count > 1 {
            Menu {
                Picker(selection: Binding { selection } set: { choose($0) }) {
                    ForEach(values, id: \.self) { value in
                        Text(verbatim: label(value)).tag(value)
                    }
                } label: {
                    Text(verbatim: title)
                }
            } label: {
                row(chooses: true)
            }
            .menuStyle(.button)
            .buttonStyle(ListRowButtonStyle())
            .accessibilityLabel(Text(verbatim: title))
            .accessibilityValue(Text(verbatim: [value, subtitle].compactMap(\.self).joined(separator: ", ")))
        } else {
            row(chooses: false)
                .accessibilityElement(children: .combine)
        }
    }

    private func row(chooses: Bool) -> some View {
        SheetRow(title: title, subtitle: subtitle, leading: leading) {
            HStack(spacing: 7) {
                accessory()
                Text(verbatim: value).appTextStyle(.body).foregroundStyle(theme.colors.inkMuted)
                    .lineLimit(typeSize.isAccessibilitySize ? nil : 1).shimmer(loading)
                if chooses {
                    Image(systemName: "chevron.up.chevron.down").font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
            }
        }
    }
}

extension ChoiceRow where Accessory == EmptyView {
    init(
        title: String, subtitle: String? = nil, value: String, loading: Bool = false, values: [Value], selection: Value,
        label: @escaping (Value) -> String, choose: @escaping (Value) -> Void, @ViewBuilder leading: @escaping () -> Leading,
    ) {
        self.init(
            title: title, subtitle: subtitle, value: value, loading: loading, values: values, selection: selection, label: label,
            choose: choose, leading: leading,
        ) { EmptyView() }
    }
}
