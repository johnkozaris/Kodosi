import SwiftUI

struct SurfaceGrain: ViewModifier {
    @Environment(\.theme) private var theme

    func body(content: Content) -> some View {
        content.overlay(
            Rectangle()
                .fill(.white.opacity(theme.isDark ? 0.03 : 0.02))
                .blendMode(theme.isDark ? .softLight : .overlay)
                .allowsHitTesting(false)
        )
    }
}

extension View {
    func surfaceGrain() -> some View {
        modifier(SurfaceGrain())
    }
}
