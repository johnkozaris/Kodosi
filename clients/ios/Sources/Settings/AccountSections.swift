import KodosiBridge
import KodosiDesign
import SwiftUI

struct AccountHeader: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    private var account: AccountStore {
        model.account
    }

    var body: some View {
        HStack(spacing: 16) {
            PersonAvatar(name: account.selfName, size: 62, isSelf: true)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 5) {
                Text(account.selfName)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(2)
                HStack(spacing: 5) {
                    Image(systemName: "lock.fill")
                        .imageScale(.small)
                    Text("Encrypted end to end")
                }
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkMuted)
            }
            Spacer(minLength: 0)
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.xl)
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("settings.account")
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
        Section {
            SettingRow(symbol: "info.circle.fill", tint: TileTint.blue, title: Text("Version")) {
                Text(verbatim: version)
                    .appTextStyle(.body)
                    .monospacedDigit()
                    .foregroundStyle(theme.colors.inkMuted)
                    .textSelection(.enabled)
            }
            .raisedRows()
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("settings.about.version")
            NavigationLink {
                LicensesView()
            } label: {
                SettingRow(symbol: "doc.text.fill", tint: TileTint.graphite, title: Text("Open-source licenses"))
            }
            .raisedRows()
            .accessibilityIdentifier("settings.about.licenses")
        } header: {
            GroupedListHeader(title: "About")
        }
    }
}

struct AccountActionsSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingSignOut = false
    @State private var confirmingDeletion = false

    private var account: AccountStore {
        model.account
    }

    private var deletionPagePresented: Binding<Bool> {
        Binding(
            get: { account.deletionPage != nil },
            set: { presented in
                if !presented {
                    account.closeDeletionPage()
                }
            }
        )
    }

    var body: some View {
        Section {
            Button { confirmingSignOut = true } label: {
                SettingRow(symbol: "rectangle.portrait.and.arrow.right", tint: TileTint.graphite, title: Text("Sign out"))
            }
            .raisedRows()
            .accessibilityIdentifier("settings.account.signOut")
            .confirmationDialog("Sign out?", isPresented: $confirmingSignOut, titleVisibility: .visible) {
                Button("Sign out", role: .destructive) { account.signOut() }
                    .accessibilityIdentifier("settings.account.confirmSignOut")
            } message: {
                Text("Terminals on your computers keep running. Sign in again to use them here.")
            }
            deletionRow
                .raisedRows()
                .sheet(isPresented: deletionPagePresented) {
                    if let page = account.deletionPage {
                        SafariView(url: page).ignoresSafeArea()
                    }
                }
        }
    }

    @ViewBuilder
    private var deletionRow: some View {
        switch account.deletion {
        case nil:
            Button { confirmingDeletion = true } label: {
                SettingRow(symbol: "trash.fill", tint: TileTint.red, title: Text("Delete my account…"), titleColor: theme.colors.danger)
            }
            .accessibilityIdentifier("settings.account.delete")
            .confirmationDialog("Delete your Kodosi account?", isPresented: $confirmingDeletion, titleVisibility: .visible) {
                Button("Delete Account", role: .destructive) { account.deleteAccount() }
                    .accessibilityIdentifier("settings.account.confirmDelete")
            } message: {
                Text(deletionSummary)
            }
        case .requested:
            SettingRow(symbol: "trash.fill", tint: TileTint.red, title: Text("Opening the account page")) {
                ProgressView()
            }
            .accessibilityIdentifier("settings.account.deletionRequested")
        case let .confirming(page):
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 12) {
                    BreathingDot(color: theme.colors.danger, size: 9)
                        .frame(width: 30)
                    Text("Confirm the deletion on the account page.")
                        .appTextStyle(.body)
                        .foregroundStyle(theme.colors.ink)
                }
                HStack(spacing: 8) {
                    if page != nil {
                        Button("Open page") { account.reopenDeletionPage() }
                            .buttonStyle(.kodosi(.secondary, size: .small))
                            .accessibilityIdentifier("settings.account.deletionPage")
                    }
                    Button("Cancel") { account.cancelAccountDeletion() }
                        .buttonStyle(.kodosi(.ghost, size: .small))
                        .accessibilityIdentifier("settings.account.deletionCancel")
                }
                .padding(.leading, 44)
            }
            .padding(.vertical, 6)
        }
    }

    private var deletionSummary: String {
        let owned = model.rooms.missions.filter { $0.ownerUserId == account.userId }.map(\.name)
        var parts = [String(localized: "Your devices, friends and sharing go away. What you wrote in other rooms stays there.")]
        if !owned.isEmpty {
            parts.append(String(localized: "These rooms close for everyone in them: \(owned.formatted(.list(type: .and)))."))
        }
        parts.append(String(localized: "Terminals on your computers keep running. This cannot be undone."))
        return parts.joined(separator: "\n\n")
    }
}
