import KodosiBridge
import KodosiDesign
import SwiftUI

struct SessionPlace: View {
    let place: TerminalPlace

    var body: some View {
        HStack(spacing: 4) {
            PlaceGlyph(kind: place.kind).font(.system(size: 9, weight: .semibold))
            Text(place.name).lineLimit(1)
        }
    }
}
