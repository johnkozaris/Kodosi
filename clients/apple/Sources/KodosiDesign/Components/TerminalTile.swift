import SwiftUI
#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// The colors of a terminal's own text and bars, the same in both appearances.
public enum TerminalInk {
    public static let body = hex(0x0D0B09)
    public static let band = hex(0x1A1512)
    public static let path = hex(0x5DAFA6)
}

public extension View {
    /// The dark tile that holds a terminal: its fill and shadow, and a hairline that turns copper
    /// while the tile is selected.
    func terminalTile(radius: CGFloat = Radius.tile, selected: Bool = false, shown: Bool = true) -> some View {
        modifier(TerminalTile(radius: radius, selected: selected, shown: shown))
    }
}

private struct TerminalTile: ViewModifier {
    @Environment(\.theme) private var theme
    let radius: CGFloat
    let selected: Bool
    let shown: Bool

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        content
            .background {
                if shown {
                    shape.fill(theme.colors.terminal)
                        .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.5 : 0.14), radius: 10, x: 3, y: 6)
                }
            }
            .overlay {
                if shown {
                    shape.strokeBorder(selected ? theme.colors.accent.mix(with: theme.colors.hairline, by: 0.55) : theme.colors.hairline.opacity(0.7),
                                       lineWidth: selected ? 1 : 0.5)
                        .allowsHitTesting(false)
                }
            }
    }
}

/// A few lines of a terminal in a font, a size and a cursor, to show what a choice looks like.
public struct TerminalPreview: View {
    public enum Cursor: Hashable, Sendable {
        case block, bar, underline
    }

    private struct Look: Equatable {
        let family: String
        let size: CGFloat
        let lineHeight: Double
        let cursor: Cursor
        let blinks: Bool
    }

    @Environment(\.theme) private var theme
    private let look: Look

    public init(family: String, size: CGFloat, lineHeight: Double = 1, cursor: Cursor = .block, blinks: Bool = true) {
        look = Look(family: family, size: size, lineHeight: lineHeight, cursor: cursor, blinks: blinks)
    }

    private var font: Font {
        installed ? .custom(look.family, size: look.size) : .system(size: look.size, design: .monospaced)
    }

    private var installed: Bool {
        #if os(macOS)
            NSFont(name: look.family, size: look.size) != nil
                || NSFontManager.shared.font(withFamily: look.family, traits: [], weight: 5, size: look.size) != nil
        #else
            UIFont(name: look.family, size: look.size) != nil || !UIFont.fontNames(forFamilyName: look.family).isEmpty
        #endif
    }

    public var body: some View {
        let size = look.size
        let ink = hex(0xE8E1D9)
        VStack(alignment: .leading, spacing: max(0, (look.lineHeight - 1) * size)) {
            HStack(spacing: size * 0.5) {
                Text(verbatim: "~/kodosi").foregroundStyle(TerminalInk.path)
                Text(verbatim: "❯").foregroundStyle(theme.colors.accent)
                Text(verbatim: "claude").foregroundStyle(ink)
            }
            Text(verbatim: "✳ Welcome back. What are we building?").foregroundStyle(hex(0xD97757))
            HStack(spacing: size * 0.5) {
                Text(verbatim: "~/kodosi").foregroundStyle(TerminalInk.path)
                Text(verbatim: "❯").foregroundStyle(theme.colors.accent)
                cursor
            }
        }
        .font(font)
        .lineLimit(1)
        .padding(18)
        .frame(maxWidth: .infinity, minHeight: 128, alignment: .topLeading)
        .background(theme.colors.terminal, in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).strokeBorder(theme.colors.hairline.opacity(0.7), lineWidth: 0.5) }
        .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.14), radius: 12, x: 3, y: 8)
        .animation(theme.motion.spring, value: look)
        .accessibilityElement(children: .ignore).accessibilityLabel(Text("Terminal preview", bundle: .module))
    }

    @ViewBuilder
    private var cursor: some View {
        let size = look.size
        switch look.cursor {
        case .block: CursorBlock(width: size * 0.6, height: size * 1.15, blinks: look.blinks).id(look.blinks)
        case .bar: CursorBlock(width: 2, height: size * 1.15, blinks: look.blinks).id(look.blinks)
        case .underline:
            CursorBlock(width: size * 0.6, height: 2, blinks: look.blinks).id(look.blinks)
                .frame(height: size * 1.15, alignment: .bottom)
        }
    }
}
