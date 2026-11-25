import SwiftUI

/// Forces a light surface even in dark mode so cameras can reliably scan the dark QR modules.
public struct ScanCodeCard<Content: View>: View {
    @Environment(\.theme) private var theme
    private let content: Content

    public init(@ContentBuilder content: () -> Content) {
        self.content = content()
    }

    public var body: some View {
        ZStack {
            Color.clear
            content
        }
        .aspectRatio(1, contentMode: .fit)
        .raised(Radius.xl, fill: AppTheme.Colors.light.raised)
        .environment(\.theme, AppTheme.light.reducingMotion(theme.motion.reduced))
        .environment(\.colorScheme, .light)
    }
}

public struct ScanCode: View {
    @Environment(\.theme) private var theme
    @Environment(\.displayScale) private var displayScale
    private let rows: [String]

    private static let quietZone = 4

    public init(rows: [String]) {
        self.rows = rows
    }

    public var body: some View {
        Canvas { context, size in
            let count = CGFloat(rows.count + 2 * Self.quietZone)
            let module = (min(size.width, size.height) * displayScale / count).rounded(.down) / displayScale
            let left = ((size.width - module * CGFloat(rows.count)) / 2 * displayScale).rounded(.down) / displayScale
            let top = ((size.height - module * CGFloat(rows.count)) / 2 * displayScale).rounded(.down) / displayScale
            var dark = Path()
            for (y, row) in rows.enumerated() {
                for (x, cell) in row.utf8.enumerated() where cell == UInt8(ascii: "1") {
                    dark.addRect(CGRect(x: left + CGFloat(x) * module, y: top + CGFloat(y) * module, width: module, height: module))
                }
            }
            context.fill(dark, with: .color(theme.colors.ink))
        }
    }
}
