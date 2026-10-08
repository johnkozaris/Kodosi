import SwiftUI

struct SessionIconButton: View {
    @State private var hovered = false
    let title: LocalizedStringKey
    let symbol: String
    let identifier: String
    var destructive = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 11, weight: .medium))
                .frame(width: 28, height: 28).contentShape(Rectangle())
        }
        .buttonStyle(SessionIconButtonStyle(hovered: hovered, destructive: destructive))
        .onHover { hovered = $0 }
        .help(title).accessibilityLabel(Text(title)).accessibilityIdentifier(identifier)
    }
}

private struct SessionIconButtonStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    @Environment(\.isEnabled) private var isEnabled
    let hovered: Bool
    let destructive: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(hovered && destructive ? theme.colors.destructive : hovered ? theme.colors.foreground : theme.colors.mutedForeground)
            .background {
                if hovered || configuration.isPressed {
                    ElevatedSurface(fill: destructive ? theme.colors.destructive.opacity(0.12) : theme.colors.secondary,
                                    isPressed: configuration.isPressed)
                }
            }
            .opacity(isEnabled ? 1 : 0.35)
            .offset(y: configuration.isPressed ? 1 : 0)
    }
}
