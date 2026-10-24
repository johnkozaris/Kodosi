import KodosiBridge
import KodosiDesign
import SwiftUI

struct SignInView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var title: LocalizedStringKey = "Sign in to work together"
    var message: LocalizedStringKey = "Your terminals stay on this Mac. Sign in to share them."

    var body: some View {
        let restoring = restoringMessage
        EmptyState(title: restoring == nil ? title : "Signing in again", message: restoring ?? message) {
            TogetherArt()
        } actions: {
            VStack(spacing: 12) {
                Button(restoring == nil ? deps.signInPrompt ?? String(localized: "Sign in") : String(localized: "Try again")) { deps.beginSignIn() }
                    .buttonStyle(.kodosi(.primary, size: .large)).accessibilityIdentifier("account.signIn")
                Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
            }
        }
    }

    private var restoringMessage: LocalizedStringKey? {
        switch deps.account.restoring {
        case "offline": "Kodosi signs you in when it reaches the server."
        case "updateRequired": "Update Kodosi to sign in."
        case "unavailable": "The Kodosi server does not answer. Kodosi tries again."
        case "unreadable": "Kodosi cannot open the Keychain, where it keeps your sign-in. Kodosi tries again."
        default: nil
        }
    }
}
