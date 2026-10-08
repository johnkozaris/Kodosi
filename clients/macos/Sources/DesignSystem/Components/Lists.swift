import SwiftUI

struct ListGroup<Content: View>: View {
    @Environment(\.theme) private var theme
    var title: LocalizedStringKey?
    var footer: LocalizedStringKey?
    @ViewBuilder let content: () -> Content

    var body: some View {
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

struct ListRow<Leading: View, Trailing: View>: View {
    @Environment(\.theme) private var theme
    let title: String
    var subtitle: String?
    var monoSubtitle = false
    @ViewBuilder let leading: () -> Leading
    @ViewBuilder let trailing: () -> Trailing

    var body: some View {
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

extension ListRow where Leading == IconTile {
    init(
        _ title: String, subtitle: String? = nil, monoSubtitle: Bool = false, symbol: String, tint: Color? = nil,
        @ViewBuilder trailing: @escaping () -> Trailing
    ) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle,
                  leading: { IconTile(symbol: symbol, tint: tint, size: 28) }, trailing: trailing)
    }
}

extension ListRow where Leading == IconTile, Trailing == EmptyView {
    init(_ title: String, subtitle: String? = nil, monoSubtitle: Bool = false, symbol: String, tint: Color? = nil) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle,
                  leading: { IconTile(symbol: symbol, tint: tint, size: 28) }, trailing: { EmptyView() })
    }
}

enum TileTint {
    static let orange = hex(0xD98A5F)
    static let amber = hex(0xD9A441)
    static let green = hex(0x5FA57E)
    static let teal = hex(0x4F9BB0)
    static let blue = hex(0x5B82C9)
    static let purple = hex(0x8C7BD1)
    static let pink = hex(0xC96F8E)
    static let red = hex(0xCF6A62)
    static let graphite = hex(0x6B6560)
}

struct PageHeader<Trailing: View>: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    var facts: String?
    @ViewBuilder let trailing: () -> Trailing

    var body: some View {
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
