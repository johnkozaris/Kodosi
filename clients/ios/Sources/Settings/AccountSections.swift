import KodosiBridge
import KodosiDesign
import SwiftUI

struct AccountHeader: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingSignOut = false

    private var account: AccountStore {
        model.account
    }

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) {
                identity
                Spacer(minLength: 0)
                signOut
            }
            VStack(alignment: .leading, spacing: 14) {
                identity
                signOut
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.xl)
        .confirmationDialog("Sign out?", isPresented: $confirmingSignOut, titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { account.signOut() }
                .accessibilityIdentifier("settings.account.confirmSignOut")
        } message: {
            Text("Terminals on your computers keep running. Sign in again to use them here.")
        }
    }

    private var identity: some View {
        HStack(spacing: 14) {
            PersonAvatar(name: account.selfName, size: 52, isSelf: true)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text(account.selfName)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(2)
                Label("Encrypted end to end", systemImage: "lock.fill")
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("settings.account")
    }

    private var signOut: some View {
        Button("Sign out…") { confirmingSignOut = true }
            .buttonStyle(.kodosi(.secondary, size: .small))
            .accessibilityIdentifier("settings.account.signOut")
    }
}

struct AboutSection: View {
    @Environment(\.theme) private var theme

    private var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "0"
        let build = info?["CFBundleVersion"] as? String ?? "0"
        return "\(short) (\(build))"
    }

    var body: some View {
        ListGroup(title: "About") {
            ListRow(String(localized: "Version"), symbol: "app.badge", tint: TileTint.blue) {
                Text(verbatim: version)
                    .appTextStyle(.body)
                    .monospacedDigit()
                    .foregroundStyle(theme.colors.inkMuted)
                    .textSelection(.enabled)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("settings.about.version")
            NavigationLink {
                LicensesView()
            } label: {
                ListRow(String(localized: "Open-source licenses"), symbol: "doc.text.fill", tint: TileTint.graphite) {
                    Disclosure()
                }
            }
            .buttonStyle(ListRowButtonStyle())
            .accessibilityIdentifier("settings.about.licenses")
            AboutLinks()
        }
    }
}

struct AccountDeletion: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirming = false

    private var account: AccountStore {
        model.account
    }

    private var pagePresented: Binding<Bool> {
        Binding(
            get: { account.deletionPage != nil },
            set: { presented in
                if !presented {
                    account.closeDeletionPage()
                }
            },
        )
    }

    var body: some View {
        Group {
            switch account.deletion {
            case nil:
                Button { confirming = true } label: {
                    Text("Delete my account…").foregroundStyle(theme.colors.danger)
                }
                .buttonStyle(.kodosi(.ghost, size: .small))
                .accessibilityIdentifier("settings.account.delete")
                .confirmationDialog("Delete your Kodosi account?", isPresented: $confirming, titleVisibility: .visible) {
                    Button("Delete Account", role: .destructive) { account.deleteAccount() }
                        .accessibilityIdentifier("settings.account.confirmDelete")
                } message: {
                    Text(summary)
                }
            case .requested:
                HStack(spacing: 10) {
                    Spinner(size: 14)
                    Text("Opening the account page").appTextStyle(.body).foregroundStyle(theme.colors.ink)
                }
                .padding(.horizontal, 16).frame(minHeight: 46)
                .raised(Radius.lg)
                .accessibilityIdentifier("settings.account.deletionRequested")
            case let .confirming(page):
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 10) {
                        BreathingDot(color: theme.colors.danger)
                        Text("Confirm the deletion on the account page.").appTextStyle(.body).foregroundStyle(theme.colors.ink)
                    }
                    HStack(spacing: 8) {
                        Spacer()
                        if page != nil {
                            Button("Open page") { account.reopenDeletionPage() }
                                .buttonStyle(.kodosi(.secondary, size: .small))
                                .accessibilityIdentifier("settings.account.deletionPage")
                        }
                        Button("Cancel") { account.cancelAccountDeletion() }
                            .buttonStyle(.kodosi(.ghost, size: .small))
                            .accessibilityIdentifier("settings.account.deletionCancel")
                    }
                }
                .padding(16)
                .raised(Radius.lg)
            }
        }
        .transition(AnyTransition.rise)
        .sheet(isPresented: pagePresented) {
            if let page = account.deletionPage {
                SafariView(url: page).ignoresSafeArea()
            }
        }
    }

    private var summary: String {
        account.deletionSummary(
            projects: model.projects.entries,
            terminals: String(localized: "Terminals on your computers keep running. You cannot undo this."),
        )
    }
}
