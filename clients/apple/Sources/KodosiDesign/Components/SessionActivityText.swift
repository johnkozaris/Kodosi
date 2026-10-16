import KodosiBridge
import SwiftUI

public struct SessionActivityText: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let fallback: String?
    let showsPlace: Bool

    public init(session: RuntimeSession, fallback: String? = nil, showsPlace: Bool = false) {
        self.session = session
        self.fallback = fallback
        self.showsPlace = showsPlace
    }

    private var place: TerminalPlace? {
        showsPlace && session.activity == nil ? session.place : nil
    }

    private var words: String? {
        session.activity ?? fallback
    }

    public var body: some View {
        HStack(spacing: 5) {
            if let place {
                HStack(spacing: 4) {
                    PlaceGlyph(kind: place.kind)
                    Text(place.name).lineLimit(1)
                }
                .id(place.name).transition(.blurReplace)
            } else if let words {
                ZStack(alignment: .leading) {
                    Text(words).lineLimit(1).shimmer(session.isWorking)
                        .id(words).transition(.blurReplace)
                }
            }
            if let progress = session.progress {
                Text(Double(progress) / 100, format: .percent).monospacedDigit().lineLimit(1)
                    .contentTransition(.numericText(value: Double(progress))).layoutPriority(1)
            }
        }
        .animation(theme.motion.soft, value: words)
        .animation(theme.motion.soft, value: place?.name)
        .animation(theme.motion.snappy, value: session.progress)
    }
}
