import SwiftUI

/// The keys in the bars above the canvas's browser and screen.
enum CanvasBar {
    #if os(iOS)
        static let key: CGFloat = 40
    #else
        static let key: CGFloat = 26
    #endif
}

struct SharedBrowserKey: View {
    @Environment(\.theme) private var theme
    let height: CGFloat

    private var explanation: String {
        String(localized: "This terminal is shared, so its browser has its own sign-ins, apart from yours. Everyone in the terminal uses them.",
               bundle: .module)
    }

    var body: some View {
        ExplainedKey(words: explanation) {
            HStack(spacing: 4) {
                Image(systemName: "person.2.fill").font(scaled: 9, weight: .semibold)
                Text("Shared", bundle: .module)
            }
            .appTextStyle(.footnote).fontWeight(.medium).foregroundStyle(theme.colors.inkMuted)
            .padding(.horizontal, 9).frame(height: height)
            .background(Capsule().fill(theme.colors.ink.opacity(0.06)))
            .contentShape(Capsule())
        }
        .accessibilityLabel(Text("Shared", bundle: .module))
        .accessibilityHint(Text(explanation))
        .accessibilityIdentifier("canvas.browser.shared")
    }
}

/// Shows explanation text in a popover on tap as well as a tooltip, since touch devices have no
/// hover tooltips.
struct ExplainedKey<Face: View>: View {
    @Environment(\.theme) private var theme
    let words: String
    @ContentBuilder let face: Face
    @State private var explaining = false

    var body: some View {
        Button { explaining.toggle() } label: { face }
            .buttonStyle(PressScaleStyle())
            .help(words)
            .popover(isPresented: $explaining) {
                Text(words).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                    .fixedSize(horizontal: false, vertical: true).frame(width: 260).padding(14)
                    .presentationCompactAdaptation(.popover)
            }
    }
}
