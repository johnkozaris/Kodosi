import KodosiDesign
import SwiftUI

struct SettingsHome: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    private var account: AccountStore {
        model.account
    }

    var body: some View {
        List {
            Section {
                AccountHeader()
                    .bareRow()
            }
            DevicesSection()
            if account.localDeviceEnrolled {
                RecoverySection()
            }
            Section {
                AppearancePicker()
                    .raisedRows()
            } header: {
                GroupedListHeader(title: "Appearance")
            }
            AboutSection()
            AccountActionsSection()
            Section {
                KodosiWordmark(size: 15, blinks: false)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                    .bareRow()
            }
        }
        .groupedListChrome()
        .navigationTitle("Settings")
        .animation(theme.motion.spring, value: account.devices)
        .animation(theme.motion.spring, value: account.deviceRequests)
        .animation(theme.motion.spring, value: account.deletion)
        .animation(theme.motion.snappy, value: account.creatingRecoveryKey)
        .refreshable { await account.refreshDevices() }
        .task { await account.refreshDevices() }
    }
}
