import KodosiBridge
import KodosiDesign
import SwiftUI

struct LauncherModes: View {
    @Environment(\.theme) private var theme
    let info: AgentInfo
    let permissions: AgentPermissions
    let output: AgentOutput
    let pick: (String) -> Void

    var body: some View {
        VStack(spacing: 0) {
            ModeRow(
                title: String(localized: "Permissions"), identifier: "launcher.permissions",
                options: info.permissions.map {
                    ModeRow.Option(id: $0.rawValue, label: $0.label, detail: $0.detail(for: info.kind), caution: $0 == .unrestricted)
                },
                current: permissions.rawValue, pick: pick,
            )
            if !info.outputs.isEmpty {
                Rectangle().fill(theme.colors.hairline.opacity(0.6)).frame(height: 0.5)
                ModeRow(
                    title: String(localized: "Output"), identifier: "launcher.output",
                    options: info.outputs.map { ModeRow.Option(id: $0.rawValue, label: $0.label, detail: $0.detail, caution: false) },
                    current: output.rawValue, pick: pick,
                )
            }
        }
        .frame(width: 300)
    }
}

struct ModeRow: View {
    struct Option: Identifiable {
        let id: String
        let label: String
        let detail: String
        let caution: Bool
        var enabled = true
    }

    @Environment(\.theme) private var theme
    @Namespace private var thumb
    let title: String
    let identifier: String
    let options: [Option]
    let current: String
    let pick: (String) -> Void
    @State private var hovered: String?

    private var shown: Option? {
        options.first { $0.id == (hovered ?? current) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(verbatim: title).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            HStack(spacing: 2) {
                ForEach(options) { option in
                    segment(option)
                }
            }
            .padding(3)
            .wellCapsule()
            .animation(theme.motion.snappy, value: current)
            Text(verbatim: shown?.detail ?? "")
                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentTransition(.interpolate)
                .animation(theme.motion.hover, value: shown?.id)
        }
        .padding(.horizontal, 14).padding(.top, 10).padding(.bottom, 12)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(verbatim: title))
        .accessibilityIdentifier(identifier)
    }

    private func segment(_ option: Option) -> some View {
        let selected = option.id == current
        let ink = option.enabled ? theme.colors.inkMuted : theme.colors.inkFaint
        return Button {
            if option.enabled {
                pick(option.id)
            }
        } label: {
            Text(verbatim: option.label).appTextStyle(.caption)
                .foregroundStyle(selected ? option.caution ? theme.colors.caution : theme.colors.ink : ink)
                .frame(maxWidth: .infinity)
                .frame(height: 26)
                .background {
                    if selected {
                        RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                            .matchedGeometryEffect(id: "thumb", in: thumb)
                    }
                }
                .contentShape(Capsule())
        }
        .buttonStyle(PressScaleStyle(scale: option.enabled ? 0.95 : 1))
        .onHover { inside in
            if inside {
                hovered = option.id
            } else if hovered == option.id {
                hovered = nil
            }
        }
        .accessibilityLabel(Text(verbatim: option.label))
        .accessibilityHint(Text(verbatim: option.detail))
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("\(identifier).\(option.id)")
    }
}
