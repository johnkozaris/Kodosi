import KodosiDesign
import SwiftUI

struct SettingRow<Trailing: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.dynamicTypeSize) private var typeSize
    let symbol: String
    var tint: Color?
    let title: Text
    var subtitle: Text?
    var titleColor: Color?
    @ViewBuilder var trailing: () -> Trailing

    init(
        symbol: String,
        tint: Color? = nil,
        title: Text,
        subtitle: Text? = nil,
        titleColor: Color? = nil,
        @ViewBuilder trailing: @escaping () -> Trailing
    ) {
        self.symbol = symbol
        self.tint = tint
        self.title = title
        self.subtitle = subtitle
        self.titleColor = titleColor
        self.trailing = trailing
    }

    var body: some View {
        Group {
            if typeSize.isAccessibilitySize {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 14) {
                        IconTile(symbol: symbol, tint: tint, size: 30)
                        titles
                        Spacer(minLength: 0)
                    }
                    trailing()
                        .padding(.leading, 44)
                }
            } else {
                HStack(spacing: 14) {
                    IconTile(symbol: symbol, tint: tint, size: 30)
                    titles
                    Spacer(minLength: 8)
                    trailing()
                }
            }
        }
        .padding(.vertical, 3)
    }

    private var titles: some View {
        VStack(alignment: .leading, spacing: 2) {
            title
                .appTextStyle(.body)
                .foregroundStyle(titleColor ?? theme.colors.ink)
            if let subtitle {
                subtitle
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .lineLimit(typeSize.isAccessibilitySize ? nil : 2)
            }
        }
    }
}

extension SettingRow where Trailing == EmptyView {
    init(symbol: String, tint: Color? = nil, title: Text, subtitle: Text? = nil, titleColor: Color? = nil) {
        self.init(symbol: symbol, tint: tint, title: title, subtitle: subtitle, titleColor: titleColor) { EmptyView() }
    }
}
