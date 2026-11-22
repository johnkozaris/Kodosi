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

    /// On iPhone and iPad, the square of a key at the end of a tab ends `reach` points after the key:
    /// it takes the end of the key's own tab and leaves the next tab all of its own. VoiceOver
    /// still outlines the key, because the square lies off its center.
    func touchTarget(_ shape: some Shape, reach: CGFloat) -> some View {
        #if os(iOS)
            contentShape(TouchArea(reach: reach)).contentShape(.accessibility, shape)
        #else
            contentShape(shape)
        #endif
    }

    /// A scroll view takes no touch beyond its edges. On iPhone and iPad, a scroll view of keys
    /// `height` points high reaches as far above and below as their touch targets do, in the same
    /// place and showing no more than before.
    func touchRoom(height: CGFloat) -> some View {
        #if os(iOS)
            let room = max(0, (TouchArea.side - height) / 2)
            return contentMargins(.vertical, room, for: .scrollContent)
                .mask { Rectangle().padding(.vertical, room) }
                .padding(.vertical, -room)
        #else
            return self
        #endif
    }
}

#if os(iOS)
    struct TouchArea: Shape {
        static let side: CGFloat = 44
        var reach: CGFloat?

        func path(in rect: CGRect) -> Path {
            var area = rect.insetBy(dx: min(0, (rect.width - Self.side) / 2), dy: min(0, (rect.height - Self.side) / 2))
            if let reach {
                area.size.width = max(rect.width + reach, Self.side)
                area.origin.x = rect.maxX + reach - area.width
            }
            return Path(area)
        }
    }
#endif
