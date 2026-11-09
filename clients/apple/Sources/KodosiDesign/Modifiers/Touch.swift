import SwiftUI

public extension View {
    /// Makes a small control at least 44 points wide and high to a finger, without changing its
    /// face. A pointer keeps the control's own shape.
    func touchTarget() -> some View {
        #if os(iOS)
            contentShape(TouchArea())
        #else
            self
        #endif
    }

    /// Makes a small control at least 44 points wide and high to a finger, and gives a pointer the
    /// control's shape.
    func touchTarget(_ shape: some Shape) -> some View {
        #if os(iOS)
            contentShape(TouchArea())
        #else
            contentShape(shape)
        #endif
    }
}

#if os(iOS)
    /// The rectangle of a control, grown around its middle to the smallest size that a finger hits.
    struct TouchArea: Shape {
        private static let side: CGFloat = 44

        func path(in rect: CGRect) -> Path {
            Path(rect.insetBy(dx: min(0, (rect.width - Self.side) / 2), dy: min(0, (rect.height - Self.side) / 2)))
        }
    }
#endif
