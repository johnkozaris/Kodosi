import KodosiDesign
import SwiftUI

struct AppearancePicker: View {
    @Environment(\.theme) private var theme
    @AppStorage(ThemePreference.storageKey) private var preference = ThemePreference.system.rawValue
    @Namespace private var selectionRing

    private var selected: ThemePreference {
        ThemePreference(rawValue: preference) ?? .system
    }

    private let options: [ThemePreference] = [.system, .light, .dark]

    var body: some View {
        HStack(spacing: 12) {
            ForEach(options, id: \.self) { option in
                let isSelected = option == selected
                Button {
                    withAnimation(theme.motion.soft) { preference = option.rawValue }
                } label: {
                    VStack(spacing: 9) {
                        ThemeSwatch(preference: option)
                            .aspectRatio(0.82, contentMode: .fit)
                            .frame(maxWidth: 96)
                            .overlay {
                                if isSelected {
                                    RoundedRectangle(cornerRadius: Radius.lg + 4, style: .continuous)
                                        .strokeBorder(theme.colors.accent, lineWidth: 2.5)
                                        .padding(-5)
                                        .matchedGeometryEffect(id: "ring", in: selectionRing)
                                }
                            }
                        HStack(spacing: 5) {
                            Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                                .foregroundStyle(isSelected ? theme.colors.accent : theme.colors.inkFaint)
                                .contentTransition(.symbolEffect(.replace))
                            Text(option.title)
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
                .accessibilityLabel(Text(option.title))
                .accessibilityAddTraits(isSelected ? .isSelected : [])
                .accessibilityIdentifier("settings.appearance.\(option.rawValue)")
            }
        }
        .padding(.vertical, 10)
        .animation(theme.motion.spring, value: selected)
        .sensoryFeedback(.selection, trigger: preference)
    }
}

private struct ThemeSwatch: View {
    let preference: ThemePreference

    var body: some View {
        switch preference {
        case .system:
            ZStack {
                SwatchFace(colors: AppTheme.light.colors)
                SwatchFace(colors: AppTheme.dark.colors)
                    .mask { DiagonalHalf() }
            }
        case .light:
            SwatchFace(colors: AppTheme.light.colors)
        case .dark:
            SwatchFace(colors: AppTheme.dark.colors)
        }
    }
}

private struct SwatchFace: View {
    let colors: AppTheme.Colors

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.lg, style: .continuous)
        shape
            .fill(colors.ground)
            .overlay(alignment: .topLeading) {
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 5) {
                        Circle().fill(colors.accent).frame(width: 10, height: 10)
                        Capsule().fill(colors.ink.opacity(0.75)).frame(width: 26, height: 4)
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        Capsule().fill(colors.ink.opacity(0.7)).frame(width: 30, height: 4)
                        Capsule().fill(colors.inkMuted.opacity(0.6)).frame(width: 20, height: 4)
                    }
                    .padding(7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(colors.raised, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                    Capsule().fill(colors.accent).frame(width: 24, height: 7)
                }
                .padding(9)
            }
            .overlay { shape.strokeBorder(colors.hairline, lineWidth: 0.75) }
            .clipShape(shape)
    }
}

private nonisolated struct DiagonalHalf: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

private extension ThemePreference {
    var title: LocalizedStringKey {
        switch self {
        case .system: "Automatic"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
}
