import SwiftUI

public struct ListGroup<Content: View>: View {
    @Environment(\.theme) private var theme
    public var title: LocalizedStringKey?
    public var footer: LocalizedStringKey?

    @ViewBuilder public let content: () -> Content

    public init(
        title: LocalizedStringKey? = nil,
        footer: LocalizedStringKey? = nil,
        @ViewBuilder content: @escaping () -> Content,
    ) {
        self.title = title
        self.footer = footer
        self.content = content
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let title {
                Text(title).appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
            }
            VStack(spacing: 0) {
                Group(subviews: content()) { subviews in
                    ForEach(Array(subviews.enumerated()), id: \.element.id) { index, subview in
                        subview
                        if index < subviews.count - 1 {
                            Rectangle().fill(theme.colors.hairline.opacity(0.55)).frame(height: 0.5).padding(.leading, 54)
                        }
                    }
                }
            }
            .raised(Radius.xl)
            if let footer {
                Text(footer).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                    .padding(.leading, 4).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

public struct ListRow<Leading: View, Trailing: View>: View {
    @Environment(\.theme) private var theme
    public let title: String
    public var subtitle: String?
    public var monoSubtitle = false

    @ViewBuilder public let leading: () -> Leading
    @ViewBuilder public let trailing: () -> Trailing

    public init(
        title: String,
        subtitle: String? = nil,
        monoSubtitle: Bool = false,
        @ViewBuilder leading: @escaping () -> Leading,
        @ViewBuilder trailing: @escaping () -> Trailing,
    ) {
        self.title = title
        self.subtitle = subtitle
        self.monoSubtitle = monoSubtitle
        self.leading = leading
        self.trailing = trailing
    }

    public var body: some View {
        HStack(spacing: 12) {
            leading().frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
                if let subtitle {
                    Text(subtitle).appTextStyle(monoSubtitle ? .monoCaption : .footnote).foregroundStyle(theme.colors.inkMuted)
                        .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                }
            }
            Spacer(minLength: 12)
            trailing()
        }
        .padding(.horizontal, 14).frame(minHeight: 52)
    }
}

public extension ListRow where Leading == IconTile {
    init(
        _ title: String, subtitle: String? = nil, monoSubtitle: Bool = false, symbol: String, tint: Color? = nil,
        @ViewBuilder trailing: @escaping () -> Trailing,
    ) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle,
                  leading: { IconTile(symbol: symbol, tint: tint, size: 28) }, trailing: trailing)
    }
}

public extension ListRow where Leading == IconTile, Trailing == EmptyView {
    init(_ title: String, subtitle: String? = nil, monoSubtitle: Bool = false, symbol: String, tint: Color? = nil) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle,
                  leading: { IconTile(symbol: symbol, tint: tint, size: 28) }, trailing: { EmptyView() })
    }
}

/// The colors of icon tiles, from the app's own: copper for what is yours or starts something,
/// amber for folders, files and care, green for what is safe, red for what removes, and
/// graphite for the rest.
public enum TileTint {
    public static let copper = hex(0xDB8A62)
    public static let amber = hex(0xDDB866)
    public static let green = hex(0x7DBB99)
    public static let red = hex(0xE77A75)
    public static let graphite = hex(0x8C7A6F)
}

public struct PageHeader<Trailing: View>: View {
    @Environment(\.theme) private var theme
    public let title: LocalizedStringKey
    public var facts: String?

    @ViewBuilder public let trailing: () -> Trailing

    public init(title: LocalizedStringKey, facts: String? = nil, @ViewBuilder trailing: @escaping () -> Trailing) {
        self.title = title
        self.facts = facts
        self.trailing = trailing
    }

    public var body: some View {
        HStack(alignment: .lastTextBaseline, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).appTextStyle(.large).foregroundStyle(theme.colors.ink)
                if let facts {
                    Text(facts).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).contentTransition(.numericText())
                }
            }
            Spacer(minLength: 12)
            trailing()
        }
    }
}
