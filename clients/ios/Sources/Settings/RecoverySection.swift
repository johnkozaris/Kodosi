import KodosiDesign
import SwiftUI

struct RecoverySection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingReplacement = false

    private var account: AccountStore {
        model.account
    }

    private var keyPresented: Binding<Bool> {
        Binding(
            get: { account.newRecoveryKey != nil },
            set: { presented in
                if !presented {
                    account.dismissRecoveryKey()
                }
            }
        )
    }

    var body: some View {
        Section {
            Button {
                if account.hasRecoveryKey {
                    confirmingReplacement = true
                } else {
                    account.createRecoveryKey()
                }
            } label: {
                SettingRow(
                    symbol: "key.fill",
                    tint: TileTint.amber,
                    title: Text(account.hasRecoveryKey ? "Make a new key…" : "Make a recovery key")
                ) {
                    if account.creatingRecoveryKey {
                        ProgressView()
                            .transition(.opacity)
                    }
                }
            }
            .disabled(account.creatingRecoveryKey)
            .raisedRows()
            .accessibilityIdentifier("settings.devices.recoveryKey")
            .confirmationDialog("Make a new recovery key?", isPresented: $confirmingReplacement, titleVisibility: .visible) {
                Button("Make a new key") { account.createRecoveryKey() }
                    .accessibilityIdentifier("settings.devices.confirmRecoveryKey")
            } message: {
                Text("The recovery key that you have now stops working.")
            }
            .sheet(isPresented: keyPresented) {
                RecoveryKeySheet(key: account.newRecoveryKey ?? "") { account.dismissRecoveryKey() }
            }
        } header: {
            GroupedListHeader(title: "If you lose all your devices")
        } footer: {
            Text("A recovery key approves a new device when you have no other device.")
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkFaint)
        }
    }
}
