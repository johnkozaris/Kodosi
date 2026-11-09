import SwiftUI

/// The name of a computer over its terminals: the computer's glyph, or the friend's face for a
/// friend's computer, and a crossed wave while the computer is away. Its parts sit in the stack
/// around it, so that a fold key can follow them.
public struct ComputerCaption: View {
    let title: String
    let ownerName: String?
    let ownerKey: String?
    let away: Bool
    let size: CGFloat
    let style: AppTextStyle

    public init(title: String, ownerName: String? = nil, ownerKey: String? = nil, away: Bool = false, size: CGFloat = 13,
                style: AppTextStyle = .caption)
    {
        self.title = title
        self.ownerName = ownerName
        self.ownerKey = ownerKey
        self.away = away
        self.size = size
        self.style = style
    }

    public var body: some View {
        if let ownerName {
            PersonAvatar(name: ownerName, key: ownerKey, size: size)
        } else {
            DeviceGlyph(label: title).font(.system(size: size * 9 / 13, weight: .semibold)).frame(width: size)
        }
        Text(title).appTextStyle(style).lineLimit(1).layoutPriority(1)
        if away {
            Image(systemName: "wifi.slash").font(.system(size: size * 8 / 13, weight: .bold))
        }
    }
}

/// Where a terminal runs, in one line: its computer, a faint chevron, then the folder or place
/// in brighter ink.
public struct TerminalLocation: View {
    @Environment(\.theme) private var theme
    let host: String?
    let glyph: String
    let name: String?

    public init(host: String?, glyph: String, name: String?) {
        self.host = host
        self.glyph = glyph
        self.name = name
    }

    public var body: some View {
        HStack(spacing: 4) {
            if let host {
                DeviceGlyph(label: host).font(.system(size: 9.5, weight: .semibold))
                Text(host).lineLimit(1)
            }
            if host != nil, name != nil {
                Image(systemName: "chevron.compact.right").font(.system(size: 11, weight: .medium))
                    .foregroundStyle(theme.colors.inkFaint).padding(.horizontal, 1)
            }
            if let name {
                HStack(spacing: 4) {
                    Image(systemName: glyph).font(.system(size: 9.5, weight: .semibold))
                    Text(name).lineLimit(1).truncationMode(.middle)
                }
                .foregroundStyle(theme.colors.ink.mix(with: theme.colors.inkMuted, by: 0.35))
            }
        }
        .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
    }

    private var label: String {
        switch (host, name) {
        case let (host?, name?): String(localized: "\(name) on \(host)", bundle: .module)
        case let (host?, nil): host
        case let (nil, name?): name
        case (nil, nil): ""
        }
    }
}

/// The face of a terminal's actions key: two copper sliders, on a copper-soft face while it is
/// lit or its menu is open.
public struct ActionsKey: View {
    @Environment(\.theme) private var theme
    let lit: Bool

    public init(lit: Bool) {
        self.lit = lit
    }

    public var body: some View {
        Image(systemName: "slider.horizontal.3")
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(theme.colors.accentStrong)
            .frame(width: 26, height: 26)
            .background {
                RoundedRectangle(cornerRadius: Radius.xs, style: .continuous)
                    .fill(lit ? theme.colors.accentSoft : .clear)
            }
            .contentShape(Rectangle())
    }
}

/// The face of a ghost key around words, such as the name of a terminal that renames it: the words
/// alone, on a quiet face while the key is lit. On iPhone and iPad it also lights while a finger
/// presses the button around it.
public struct GhostKeyFace<Content: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.bezelPressed) private var pressed
    let lit: Bool
    let content: Content

    public init(lit: Bool = false, @ViewBuilder content: () -> Content) {
        self.lit = lit
        self.content = content()
    }

    private var shown: Bool {
        #if os(iOS)
            lit || pressed
        #else
            lit
        #endif
    }

    public var body: some View {
        content.padding(.horizontal, 6).controlHeight(24)
            .background {
                if shown {
                    RoundedRectangle(cornerRadius: Radius.xs, style: .continuous).fill(theme.buttonFace(lit: true))
                }
            }
            .padding(.horizontal, -6)
            .contentShape(Rectangle())
    }
}
