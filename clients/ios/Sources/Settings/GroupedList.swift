import KodosiDesign
import SwiftUI

struct GroupedListChrome: ViewModifier {
    @State private var width: CGFloat = 0
    let readableWidth: CGFloat

    private var margin: CGFloat? {
        width > readableWidth + 200 ? (width - readableWidth) / 2 : nil
    }

    func body(content: Content) -> some View {
        content
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background { GroundBackdrop() }
            .contentMargins(.horizontal, margin, for: .scrollContent)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }
}

struct GroupedListHeader: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    var count: Int?

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
            if let count, count > 0 {
                Text(count, format: .number)
                    .monospacedDigit()
                    .foregroundStyle(theme.colors.inkFaint)
                    .contentTransition(.numericText(value: Double(count)))
            }
        }
        .appTextStyle(.subhead)
        .foregroundStyle(theme.colors.inkMuted)
        .textCase(nil)
        .animation(theme.motion.snappy, value: count)
    }
}

struct RaisedRows: ViewModifier {
    @Environment(\.theme) private var theme
    var fill: Color?

    func body(content: Content) -> some View {
        content
            .listRowBackground(fill ?? theme.colors.raised)
            .listRowSeparatorTint(theme.colors.hairline)
    }
}

extension View {
    func groupedListChrome(readableWidth: CGFloat = 720) -> some View {
        modifier(GroupedListChrome(readableWidth: readableWidth))
    }

    func raisedRows(fill: Color? = nil) -> some View {
        modifier(RaisedRows(fill: fill))
    }

    func bareRow() -> some View {
        listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())
            .listRowSeparator(.hidden)
    }
}
