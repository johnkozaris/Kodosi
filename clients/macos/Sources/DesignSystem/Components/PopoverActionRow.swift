import SwiftUI

struct PopoverActionRow: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    let icon: String
    let title: String

    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.colors.primary)
                .frame(width: 24)
                .accessibilityHidden(true)

            Text(title)
                .appTextStyle(.headingItem)
                .foregroundStyle(theme.colors.foreground)
                .lineLimit(1)

            Spacer(minLength: 8)
        }
        .padding(.horizontal, 8)
        .frame(minHeight: 38)
        .background(
            RoundedRectangle(cornerRadius: theme.radius.sm, style: .continuous)
                .fill(theme.colors.secondary.opacity(isHovered ? 0.38 : 0))
        )
        .contentShape(Rectangle())
        .onHover { isHovered = $0 }
        .animation(
            reduceMotion ? nil : .easeOut(duration: theme.motion.fast),
            value: isHovered
        )
    }
}
