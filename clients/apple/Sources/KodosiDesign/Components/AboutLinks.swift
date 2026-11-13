import SwiftUI

public struct AboutLinks: View {
    @Environment(\.theme) private var theme

    public init() {}

    public var body: some View {
        link(String(localized: "Privacy policy", bundle: .module), to: "https://kodosi.com/privacy", symbol: "hand.raised.fill",
             tint: TileTint.blue, identifier: "settings.about.privacy")
        link(String(localized: "Terms of use", bundle: .module), to: "https://kodosi.com/terms", symbol: "doc.plaintext.fill",
             tint: TileTint.graphite, identifier: "settings.about.terms")
        link(String(localized: "Support", bundle: .module), to: "https://kodosi.com/support", symbol: "lifepreserver.fill",
             tint: TileTint.green, identifier: "settings.about.support")
    }

    private func link(_ title: String, to address: String, symbol: String, tint: Color, identifier: String) -> some View {
        Link(destination: URL(string: address)!) {
            ListRow(title, symbol: symbol, tint: tint) {
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(theme.colors.inkFaint)
                    .accessibilityHidden(true)
            }
        }
        .buttonStyle(ListRowButtonStyle())
        .accessibilityIdentifier(identifier)
    }
}
