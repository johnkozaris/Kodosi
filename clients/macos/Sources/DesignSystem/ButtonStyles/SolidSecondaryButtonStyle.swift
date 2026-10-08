import SwiftUI

struct SolidSecondaryButtonStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovered = false
    var height: CGFloat?

    init(height: CGFloat? = nil) {
        self.height = height
    }

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .appTextStyle(.button)
            .foregroundStyle(theme.colors.foreground)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(minHeight: height)
            .background {
                ElevatedSurface(
                    fill: configuration.isPressed || hovered ? theme.colors.secondary : theme.colors.card,
                    isPressed: configuration.isPressed
                )
            }
            .offset(y: isEnabled && configuration.isPressed ? 1 : 0)
            .opacity(isEnabled ? 1 : 0.5)
            .contentShape(Rectangle())
            .onHover { hovered = $0 }
    }
}
