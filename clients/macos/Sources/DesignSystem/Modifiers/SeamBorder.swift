import SwiftUI

struct SeamBorder: ViewModifier {
    @Environment(\.theme) private var theme
    let edges: Edge.Set

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if edges.contains(.bottom) {
                Rectangle().fill(theme.colors.seam).frame(height: 1).frame(maxWidth: .infinity)
            }
        }
        .overlay(alignment: .top) {
            if edges.contains(.top) {
                Rectangle().fill(theme.colors.seam).frame(height: 1).frame(maxWidth: .infinity)
            }
        }
        .overlay(alignment: .trailing) {
            if edges.contains(.trailing) {
                Rectangle().fill(theme.colors.seam).frame(width: 1).frame(maxHeight: .infinity)
            }
        }
        .overlay(alignment: .leading) {
            if edges.contains(.leading) {
                Rectangle().fill(theme.colors.seam).frame(width: 1).frame(maxHeight: .infinity)
            }
        }
    }
}

extension View {
    func seamBorder(_ edges: Edge.Set) -> some View {
        modifier(SeamBorder(edges: edges))
    }
}
