import KodosiBridge
import SwiftUI

public struct SessionPlace: View {
    @Environment(\.font) private var font
    let place: TerminalPlace
    let glyphFont: Font?

    public init(place: TerminalPlace, glyphFont: Font? = nil) {
        self.place = place
        self.glyphFont = glyphFont
    }

    public var body: some View {
        HStack(spacing: 4) {
            PlaceGlyph(kind: place.kind).font(glyphFont ?? font)
            Text(place.name).lineLimit(1)
        }
    }
}
