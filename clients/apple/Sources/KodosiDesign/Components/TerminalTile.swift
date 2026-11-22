import SwiftUI
#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

public extension View {
    func terminalTile(radius: CGFloat = Radius.tile, selected: Bool = false, shown: Bool = true, fill: Color? = nil) -> some View {
        modifier(TerminalTile(radius: radius, selected: selected, shown: shown, fill: fill))
    }
}

/// A tile casts its shadow on the ground around it, so the shadow is the ground's.
private struct TerminalTile: ViewModifier {
    @Environment(\.theme) private var theme
    @Environment(\.chromeTheme) private var chrome
    let radius: CGFloat
    let selected: Bool
    let shown: Bool
    let fill: Color?

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        let ground = chrome ?? theme
        content
            .background {
                if shown {
                    shape.fill(fill ?? theme.colors.terminal)
                        .shadow(color: ground.colors.shadow.opacity(ground.isDark ? 0.5 : 0.14), radius: 10, x: 3, y: 6)
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
    @Environment(\.chromeTheme) private var chrome
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

    /// The foreground of the terminal palette of the look.
    private var ink: Color {
        theme.isDark ? hex(0xE8E1D9) : hex(0x2A1D15)
    }

    /// The cyan of the terminal palette of the look.
    private var path: Color {
        theme.isDark ? hex(0x5DAFA6) : hex(0x1F7A74)
    }

    public var body: some View {
        let size = look.size
        let ground = chrome ?? theme
        VStack(alignment: .leading, spacing: max(0, (look.lineHeight - 1) * size)) {
            HStack(spacing: size * 0.5) {
                Text(verbatim: "~/kodosi").foregroundStyle(path)
                Text(verbatim: "❯").foregroundStyle(theme.colors.accent)
                Text(verbatim: "claude").foregroundStyle(ink)
            }
            Text(verbatim: "✳ Welcome back. What are we building?").foregroundStyle(hex(0xD97757))
            HStack(spacing: size * 0.5) {
                Text(verbatim: "~/kodosi").foregroundStyle(path)
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
        .shadow(color: ground.colors.shadow.opacity(ground.isDark ? 0.45 : 0.14), radius: 12, x: 3, y: 8)
        .animation(theme.motion.spring, value: look)
        .accessibilityElement(children: .ignore).accessibilityLabel(Text("Terminal preview", bundle: .module))
    }

    @ContentBuilder
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
