import SwiftUI

struct PopoverMenuButtonStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    @State private var hovered = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label.appTextStyle(.headingItem)
            .foregroundStyle(theme.colors.foreground)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10).frame(minHeight: 38)
            .background(RoundedRectangle(cornerRadius: theme.radius.sm)
                .fill(hovered || configuration.isPressed ? theme.colors.secondary : theme.colors.card.opacity(0.6)))
            .overlay(RoundedRectangle(cornerRadius: theme.radius.sm).stroke(theme.colors.border, lineWidth: 1))
            .contentShape(Rectangle()).onHover { hovered = $0 }
    }
}
