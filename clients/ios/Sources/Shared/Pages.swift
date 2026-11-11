import KodosiDesign
import SwiftUI

extension View {
    func kodosiSheet() -> some View {
        modifier(KodosiSheet())
    }

    func readableWidth(_ width: CGFloat = 760) -> some View {
        modifier(ReadableWidth(width: width))
    }

    /// The line stays in the bar while it is empty: the bar must have its height from the start,
    /// or the page opens scrolled under it.
    func pageFacts(_ facts: String) -> some View {
        toolbar {
            ToolbarItem(placement: .largeSubtitle) { PageFacts(text: facts) }
        }
    }
}

private struct PageFacts: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        Text(text)
            .appTextStyle(.footnote)
            .foregroundStyle(theme.colors.inkMuted)
            .lineLimit(1)
            .contentTransition(.numericText())
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityHidden(text.isEmpty)
    }
}

private struct KodosiSheet: ViewModifier {
    @Environment(\.theme) private var theme

    func body(content: Content) -> some View {
        content
            .presentationBackground(theme.colors.surface)
            .presentationCornerRadius(Radius.sheet)
    }
}

private struct ReadableWidth: ViewModifier {
    @State private var room: CGFloat = 0
    let width: CGFloat

    private var margin: CGFloat? {
        room > width + 200 ? (room - width) / 2 : nil
    }

    func body(content: Content) -> some View {
        content
            .contentMargins(.horizontal, margin, for: .scrollContent)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { room = $0 }
    }
}

struct ListHeader: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    var count: Int?

    var body: some View {
        HStack(spacing: 6) {
            Text(title).appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted)
            if let count, count > 0 {
                Text(count, format: .number)
                    .appTextStyle(.footnote)
                    .monospacedDigit()
                    .foregroundStyle(theme.colors.inkFaint)
                    .contentTransition(.numericText(value: Double(count)))
            }
            Spacer(minLength: 0)
        }
        .animation(theme.motion.snappy, value: count)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}
