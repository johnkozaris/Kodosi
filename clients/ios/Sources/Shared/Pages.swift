import KodosiDesign
import SwiftUI

extension View {
    func readableWidth(_ width: CGFloat = 760) -> some View {
        modifier(ReadableWidth(width: width))
    }

    func pageFacts(_ facts: String?) -> some View {
        navigationSubtitle(facts ?? "")
    }

    /// On regular-width iPad, the top tab bar already titles root pages and a navigation title
    /// would sit at the screen edge, misaligned with the centered content column.
    func tabPage(_ title: LocalizedStringKey, inline: Bool = false, facts: String? = nil) -> some View {
        modifier(TabPage(title: title, inline: inline, facts: facts))
    }

    /// Keeps the search bar always visible so the navigation bar height never changes while
    /// scrolling (see `TabPage`).
    func pageSearch(_ text: Binding<String>, prompt: Text) -> some View {
        searchable(text: text, placement: .navigationBarDrawer(displayMode: .always), prompt: prompt)
    }
}

private struct TabPage: ViewModifier {
    @Environment(\.horizontalSizeClass) private var sizeClass
    let title: LocalizedStringKey
    let inline: Bool
    let facts: String?

    private var namedByTabBar: Bool {
        ThisDevice.isPad && sizeClass == .regular
    }

    func body(content: Content) -> some View {
        content
            .navigationTitle(title)
            // Avoid collapsing navigation titles or search bars: when popping back to a list
            // scrolled to the bottom, changing bar height and scroll insets can feedback-loop
            // and hang the app.
            .toolbarTitleDisplayMode(inline || namedByTabBar ? .inline : .inlineLarge)
            .toolbar(removing: namedByTabBar ? .title : nil)
            .pageFacts(namedByTabBar ? nil : facts)
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
            }
            Spacer(minLength: 0)
        }
        .animation(theme.motion.snappy, value: count)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}
