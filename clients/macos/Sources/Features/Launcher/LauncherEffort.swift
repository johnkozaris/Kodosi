import KodosiDesign
import SwiftUI

struct EffortBars: View {
    @Environment(\.theme) private var theme
    let level: Int?
    let count: Int
    var height: CGFloat = 11
    var width: CGFloat = 2.5

    var body: some View {
        HStack(alignment: .bottom, spacing: width * 0.6) {
            ForEach(0 ..< count, id: \.self) { index in
                RoundedRectangle(cornerRadius: width * 0.4, style: .continuous)
                    .fill(level.map { index <= $0 } == true ? theme.colors.accent : theme.colors.ink.opacity(0.2))
                    .frame(width: width, height: height * (0.35 + 0.65 * CGFloat(index) / CGFloat(max(count - 1, 1))))
                    .animation(theme.motion.snappy?.delay(Double(index) * 0.035), value: level)
            }
        }
        .frame(height: height, alignment: .bottom)
        .accessibilityHidden(true)
    }
}

struct EffortMeter: View {
    @Environment(\.theme) private var theme
    @Namespace private var thumb
    let levels: [AgentOption]
    let current: String?
    let pick: (String?) -> Void
    @State private var hovered: String?

    private var level: Int? {
        levels.firstIndex { $0.argument == current }
    }

    private var shown: String {
        let argument = hovered ?? current ?? LauncherMenu.standard
        return levels.first { $0.argument == argument }?.label ?? String(localized: "Model's own level")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Effort").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                Spacer()
                Text(verbatim: shown).appTextStyle(.footnote).fontWeight(.medium).foregroundStyle(theme.colors.inkMuted)
                    .contentTransition(.interpolate)
                    .animation(theme.motion.hover, value: shown)
            }
            HStack(spacing: 2) {
                segment(nil, label: String(localized: "Auto")) {
                    Text("Auto").appTextStyle(.caption).padding(.horizontal, 6)
                }
                ForEach(Array(levels.enumerated()), id: \.element.id) { index, option in
                    segment(option.argument, label: option.label) {
                        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                            .fill(level.map { index <= $0 } == true ? theme.colors.accent : theme.colors.ink.opacity(0.22))
                            .frame(width: 6, height: 5 + 11 * CGFloat(index) / CGFloat(max(levels.count - 1, 1)))
                            .frame(height: 18, alignment: .bottom)
                            .animation(theme.motion.snappy?.delay(Double(index) * 0.035), value: level)
                    }
                }
            }
            .padding(3)
            .wellCapsule()
        }
        .padding(.horizontal, 14).padding(.top, 10).padding(.bottom, 12)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text("Effort"))
        .accessibilityIdentifier("launcher.effort")
    }

    private func segment(_ argument: String?, label: String, @ViewBuilder content: () -> some View) -> some View {
        let selected = argument == current
        return Button { pick(argument) } label: {
            content()
                .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
                .frame(minWidth: 30, maxWidth: argument == nil ? nil : .infinity)
                .frame(height: 26)
                .background {
                    if selected {
                        RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                            .matchedGeometryEffect(id: "thumb", in: thumb)
                    }
                }
                .contentShape(Capsule())
        }
        .buttonStyle(PressScaleStyle(scale: 0.92))
        .onHover { inside in
            if inside {
                hovered = argument ?? LauncherMenu.standard
            } else if hovered == (argument ?? LauncherMenu.standard) {
                hovered = nil
            }
        }
        .help(label)
        .accessibilityLabel(Text(verbatim: label))
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("launcher.effort.\(AccessibilityIdentifier.token(argument ?? LauncherMenu.standard))")
    }
}
