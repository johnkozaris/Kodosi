import SwiftUI

struct SolidPrimaryButtonStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .appTextStyle(.button)
            .foregroundStyle(theme.colors.primaryForeground)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background {
                ElevatedSurface(
                    fill: configuration.isPressed || hovered ? theme.colors.accent : theme.colors.primary,
                    isPressed: configuration.isPressed
                )
            }
            .offset(y: configuration.isPressed ? 1 : 0)
            .opacity(isEnabled ? 1 : 0.5)
            .contentShape(Rectangle())
            .onHover { hovered = $0 }
    }
}
