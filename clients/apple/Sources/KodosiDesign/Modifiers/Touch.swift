import SwiftUI

public extension View {
    func touchTarget() -> some View {
        #if os(iOS)
            contentShape(TouchArea())
        #else
            self
        #endif
    }

    func touchTarget(_ shape: some Shape) -> some View {
        #if os(iOS)
            contentShape(TouchArea())
        #else
            contentShape(shape)
        #endif
    }
}

#if os(iOS)
    struct TouchArea: Shape {
        private static let side: CGFloat = 44

        func path(in rect: CGRect) -> Path {
            Path(rect.insetBy(dx: min(0, (rect.width - Self.side) / 2), dy: min(0, (rect.height - Self.side) / 2)))
        }
    }
#endif
