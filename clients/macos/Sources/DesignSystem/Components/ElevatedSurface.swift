import SwiftUI

struct ElevatedSurface: View {
    @Environment(\.theme) private var theme
    let fill: Color
    let isPressed: Bool

    var body: some View {
        shape
            .fill(fill)
            .overlay {
                shape
                    .stroke(rimLight, lineWidth: 1)
                    .mask(
                        LinearGradient(
                            colors: [.white, .clear],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )
            }
            .shadow(
                color: theme.colors.shadowColor.opacity(contactShadowOpacity),
                radius: isPressed ? 1 : 2,
                y: isPressed ? 1 : 2
            )
            .shadow(
                color: theme.colors.shadowColor.opacity(ambientShadowOpacity),
                radius: isPressed ? 8 : 15,
                x: isPressed ? 1 : 2,
                y: isPressed ? 3 : 10
            )
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: theme.radius.sm, style: .continuous)
    }

    private var rimLight: Color {
        theme.isDark
            ? theme.colors.foreground.opacity(0.12)
            : theme.colors.surfaceStage.opacity(0.95)
    }

    private var contactShadowOpacity: Double {
        if theme.isDark {
            return isPressed ? 0.32 : 0.50
        }
        return isPressed ? 0.10 : 0.16
    }

    private var ambientShadowOpacity: Double {
        if theme.isDark {
            return isPressed ? 0.20 : 0.38
        }
        return isPressed ? 0.06 : 0.10
    }
}
