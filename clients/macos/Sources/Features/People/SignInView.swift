import SwiftUI

struct SignInView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Image(systemName: deps.userId == nil ? "person.2.crop.square.stack" : "laptopcomputer")
                .font(.system(size: 42, weight: .ultraLight)).foregroundStyle(theme.colors.primary)
            Button(deps.signInPrompt ?? String(localized: "Sign in")) {
                deps.beginSignIn()
            }
            .buttonStyle(SolidPrimaryButtonStyle()).accessibilityIdentifier("account.signIn")
        }
    }
}
