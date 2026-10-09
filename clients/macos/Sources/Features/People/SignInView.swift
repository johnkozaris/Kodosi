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

struct TogetherArt: View {
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: -10) {
            PersonAvatar(name: "A", key: "art.a", size: 46, ring: theme.colors.surface)
            AgentMark(kind: .claude, size: 54).zIndex(1)
                .overlay { RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(theme.colors.surface, lineWidth: 3).padding(-1.5) }
            AgentMark(kind: .shell, size: 46)
                .overlay { RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(theme.colors.surface, lineWidth: 3).padding(-1.5) }
            PersonAvatar(name: "B", key: "art.d", size: 46, ring: theme.colors.surface)
        }
        .accessibilityHidden(true)
    }
}
