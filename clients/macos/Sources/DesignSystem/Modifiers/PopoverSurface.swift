import SwiftUI

struct PopoverSurface: ViewModifier {
    enum Layer {
        case canvas
        case panel
    }

    @Environment(\.theme) private var theme
    let layer: Layer

    func body(content: Content) -> some View {
        content
            .background(layer == .panel ? theme.colors.surfacePanel : theme.colors.background)
            .surfaceGrain()
    }
}

extension View {
    func popoverSurface(_ layer: PopoverSurface.Layer = .canvas) -> some View {
        modifier(PopoverSurface(layer: layer))
    }
}
