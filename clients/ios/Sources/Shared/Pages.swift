import KodosiDesign
import SwiftUI

extension View {
    func kodosiSheet() -> some View {
        modifier(KodosiSheet())
    }

    func readableWidth(_ width: CGFloat = 760) -> some View {
        modifier(ReadableWidth(width: width))
    }

    func pageFacts(_ facts: String?) -> some View {
        navigationSubtitle(facts ?? "")
    }

    /// On an iPad, the tab bar at the top names the first page of a tab, and the page's own title
    /// would stand at the edge of the screen, out of line with the page's column.
    func tabPage(_ title: LocalizedStringKey, inline: Bool = false, facts: String? = nil) -> some View {
        modifier(TabPage(title: title, inline: inline, facts: facts))
    }

    /// The field stays in view so that the bar keeps one height, for the reason in `TabPage`.
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
            // A title or search field that folds as the list scrolls makes the bar's height follow
            // the list's offset. When going back changes the insets of a list scrolled to its end,
            // the bar and the list then adjust each other without end, and the app hangs.
            .toolbarTitleDisplayMode(inline || namedByTabBar ? .inline : .inlineLarge)
            .toolbar(removing: namedByTabBar ? .title : nil)
            .pageFacts(namedByTabBar ? nil : facts)
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
            }
            Spacer(minLength: 0)
        }
        .animation(theme.motion.snappy, value: count)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}
