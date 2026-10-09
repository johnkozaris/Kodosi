import SwiftUI

struct SessionActivityText: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    var fallback: String?

    private var words: String? {
        session.activity ?? fallback
    }

    var body: some View {
        HStack(spacing: 5) {
            if let words {
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
        .animation(theme.motion.snappy, value: session.progress)
    }
}
