import KodosiBridge
import SwiftUI

public struct SessionActivityText: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let fallback: String?
    let showsPlace: Bool
    let naming: Bool

    /// Set `naming` when the primary title shows the agent summary so this subtitle shows the
    /// terminal name instead.
    public init(session: RuntimeSession, fallback: String? = nil, showsPlace: Bool = false, naming: Bool = false) {
        self.session = session
        self.fallback = fallback
        self.showsPlace = showsPlace
        self.naming = naming
    }

    private var place: TerminalPlace? {
        showsPlace && session.activity == nil ? session.place : nil
    }

    private var words: String? {
        naming ? session.name : session.activity ?? fallback
    }

    public var body: some View {
        HStack(spacing: 5) {
            if place != nil || words != nil {
                ZStack(alignment: .leading) {
                    if let place {
                        SessionPlace(place: place).id(place.name).transition(AnyTransition.roll)
                    } else if let words {
                        Text(words).lineLimit(1).id(words).transition(AnyTransition.roll)
                    }
                }
                .clipped()
            }
            if let progress = session.progress {
                Text(Double(progress) / 100, format: .percent).monospacedDigit().lineLimit(1)
                    .layoutPriority(1)
            }
        }
        .animation(theme.motion.snappy, value: words)
        .animation(theme.motion.snappy, value: place?.name)
        .animation(theme.motion.snappy, value: session.progress)
    }
}
