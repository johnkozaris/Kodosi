import KodosiBridge
import KodosiDesign
import SwiftUI

struct SettingsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    private var account: AccountStore {
        model.account
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                AccountHeader()
                DevicesSection()
                if account.localDeviceEnrolled {
                    RecoverySection()
                }
                TerminalSection()
                DocumentsSection()
                NotificationsSection()
                PrivacySection()
                LanguageSection()
                AboutSection()
                AccountDeletion()
                KodosiWordmark(size: 15, blinks: false)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
            }
            .settingsPage()
        }
        .background(theme.colors.surface)
        .navigationTitle("Settings")
        .animation(theme.motion.spring, value: account.devices)
        .animation(theme.motion.spring, value: account.deviceRequests)
        .animation(theme.motion.spring, value: account.pendingReset)
        .animation(theme.motion.spring, value: account.deletion)
        .animation(theme.motion.snappy, value: account.creatingRecoveryKey)
        .refreshable { await account.refreshDevices() }
        .task { await account.refreshDevices() }
    }
}

extension View {
    /// A page of settings: one column of groups, at most as wide as a line reads well.
    func settingsPage() -> some View {
        padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 28)
            .frame(maxWidth: 760)
            .frame(maxWidth: .infinity)
    }
}
