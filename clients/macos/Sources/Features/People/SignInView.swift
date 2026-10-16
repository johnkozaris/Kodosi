import KodosiDesign
import SwiftUI

struct SignInView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var title: LocalizedStringKey = "Sign in to work together"
    var message: LocalizedStringKey = "Your terminals stay on this Mac. Sign in to share them."

    var body: some View {
        EmptyState(title: title, message: message) {
            TogetherArt()
        } actions: {
            VStack(spacing: 12) {
                Button(deps.signInPrompt ?? String(localized: "Sign in")) { deps.beginSignIn() }
                    .buttonStyle(.kodosi(.primary, size: .large)).accessibilityIdentifier("account.signIn")
                Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            }
        }
    }
}
