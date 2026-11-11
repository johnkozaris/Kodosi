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
            #if os(iOS)
            .clipShape(RoundedRectangle(cornerRadius: Radius.xl, style: .continuous))
            #endif
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
    #if os(iOS)
        @Environment(\.dynamicTypeSize) private var typeSize
    #endif
    public let title: String
    public var subtitle: String?
    public var monoSubtitle = false
    public var titleColor: Color?

    @ViewBuilder public let leading: () -> Leading
    @ViewBuilder public let trailing: () -> Trailing

    public init(
        title: String,
        subtitle: String? = nil,
        monoSubtitle: Bool = false,
        titleColor: Color? = nil,
        @ViewBuilder leading: @escaping () -> Leading,
        @ViewBuilder trailing: @escaping () -> Trailing,
    ) {
        self.title = title
        self.subtitle = subtitle
        self.monoSubtitle = monoSubtitle
        self.titleColor = titleColor
        self.leading = leading
        self.trailing = trailing
    }

    public var body: some View {
        #if os(iOS)
            ViewThatFits(in: .horizontal) {
                if !typeSize.isAccessibilitySize {
                    HStack(spacing: 12) {
                        leading().frame(width: 28)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(title).appTextStyle(.body).foregroundStyle(titleColor ?? theme.colors.ink).lineLimit(1)
                            if let subtitle {
                                subtitleText(subtitle).lineLimit(2).frame(idealWidth: 0, maxWidth: .infinity, alignment: .leading)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        trailing().fixedSize()
                    }
                }
                HStack(alignment: .top, spacing: 12) {
                    leading().frame(width: 28)
                    VStack(alignment: .leading, spacing: 10) {
                        titles(lines: nil)
                        trailing()
                    }
                    Spacer(minLength: 0)
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 8).frame(minHeight: 52)
        #else
            HStack(spacing: 12) {
                leading().frame(width: 28)
                titles(lines: 1)
                Spacer(minLength: 12)
                trailing()
            }
            .padding(.horizontal, 14).frame(minHeight: 52)
        #endif
    }

    private func titles(lines: Int?) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).appTextStyle(.body).foregroundStyle(titleColor ?? theme.colors.ink).lineLimit(lines)
            if let subtitle {
                subtitleText(subtitle).lineLimit(lines)
            }
        }
    }

    private func subtitleText(_ subtitle: String) -> some View {
        Text(subtitle).appTextStyle(monoSubtitle ? .monoCaption : .footnote).foregroundStyle(theme.colors.inkMuted)
            .truncationMode(.middle).textSelection(.enabled)
    }
}

public extension ListRow where Leading == IconTile {
    init(
        _ title: String, subtitle: String? = nil, monoSubtitle: Bool = false, symbol: String, tint: Color? = nil, titleColor: Color? = nil,
        @ViewBuilder trailing: @escaping () -> Trailing,
    ) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle, titleColor: titleColor,
                  leading: { IconTile(symbol: symbol, tint: tint, size: 28) }, trailing: trailing)
    }
}

public extension ListRow where Leading == IconTile, Trailing == EmptyView {
    init(_ title: String, subtitle: String? = nil, monoSubtitle: Bool = false, symbol: String, tint: Color? = nil, titleColor: Color? = nil) {
        self.init(title: title, subtitle: subtitle, monoSubtitle: monoSubtitle, titleColor: titleColor,
                  leading: { IconTile(symbol: symbol, tint: tint, size: 28) }, trailing: { EmptyView() })
    }
}

public struct SwitchRow: View {
    @Environment(\.theme) private var theme
    public let title: String
    public var subtitle: String?
    public let symbol: String
    public var tint: Color?
    @Binding public var isOn: Bool

    public init(_ title: String, subtitle: String? = nil, symbol: String, tint: Color? = nil, isOn: Binding<Bool>) {
        self.title = title
        self.subtitle = subtitle
        self.symbol = symbol
        self.tint = tint
        _isOn = isOn
    }

    public var body: some View {
        Toggle(isOn: $isOn) {
            HStack(spacing: 12) {
                IconTile(symbol: symbol, tint: tint, size: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                    if let subtitle {
                        Text(subtitle).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                }
            }
        }
        .toggleStyle(KodosiSwitchStyle())
        .padding(.horizontal, 14)
        #if os(iOS)
            .padding(.vertical, 8)
        #endif
            .frame(minHeight: 52)
    }
}

public struct ListRowButtonStyle: ButtonStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        RowFace(label: configuration.label, pressed: configuration.isPressed)
    }
}

private struct RowFace<Label: View>: View {
    @Environment(\.theme) private var theme
    let label: Label
    let pressed: Bool

    var body: some View {
        label
            .background { theme.colors.lifted.opacity(pressed ? 1 : 0) }
            .contentShape(Rectangle())
            .animation(theme.motion.hover, value: pressed)
    }
}

public struct Disclosure: View {
    @Environment(\.theme) private var theme

    public init() {}

    public var body: some View {
        Image(systemName: "chevron.right").font(.system(size: 12, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
            .accessibilityHidden(true)
    }
}

public enum TileTint {
    public static let orange = hex(0xD98A5F)
    public static let amber = hex(0xD9A441)
    public static let green = hex(0x5FA57E)
    public static let teal = hex(0x4F9BB0)
    public static let blue = hex(0x5B82C9)
    public static let purple = hex(0x8C7BD1)
    public static let pink = hex(0xC96F8E)
    public static let red = hex(0xCF6A62)
    public static let graphite = hex(0x6B6560)
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
