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

    /// On iOS, caps the trailing edge of the 44pt hit target at `reach` points past the view so
    /// a trailing button (e.g. a tab close button) does not overlap the adjacent item. Keeps the
    /// VoiceOver outline on `shape` instead of the off-center hit area.
    func touchTarget(_ shape: some Shape, reach: CGFloat) -> some View {
        #if os(iOS)
            contentShape(TouchArea(reach: reach)).contentShape(.accessibility, shape)
        #else
            contentShape(shape)
        #endif
    }

    /// A scroll view takes no touches beyond its edges. On iOS, this expands a scroll view's
    /// vertical hit area to cover 44pt touch targets for controls of `height`, while masking
    /// visuals and using negative padding to keep layout unchanged.
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
