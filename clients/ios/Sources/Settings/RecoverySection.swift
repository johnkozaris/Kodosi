import KodosiBridge
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
            },
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("If you lose all your devices").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) {
                    words
                    Spacer(minLength: 8)
                    key
                }
                VStack(alignment: .leading, spacing: 12) {
                    words
                    key
                }
            }
            .padding(.horizontal, 16).padding(.vertical, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .raised(Radius.lg)
        }
        .confirmationDialog("Make a new recovery key?", isPresented: $confirmingReplacement, titleVisibility: .visible) {
            Button("Make a new key") { account.createRecoveryKey() }
                .accessibilityIdentifier("settings.devices.confirmRecoveryKey")
        } message: {
            Text("The recovery key that you have now stops working.")
        }
        .sheet(isPresented: keyPresented) {
            RecoveryKeySheet(key: account.newRecoveryKey ?? "") { account.dismissRecoveryKey() }
        }
    }

    private var words: some View {
        Text("A recovery key approves a new device when you have no other device.")
            .appTextStyle(.footnote)
            .foregroundStyle(theme.colors.inkMuted)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var key: some View {
        Button {
            if account.hasRecoveryKey {
                confirmingReplacement = true
            } else {
                account.createRecoveryKey()
            }
        } label: {
            HStack(spacing: 8) {
                if account.creatingRecoveryKey {
                    Spinner(size: 12)
                }
                Text(account.hasRecoveryKey ? "Make a new key…" : "Make a recovery key")
            }
        }
        .disabled(account.creatingRecoveryKey)
        .accessibilityIdentifier("settings.devices.recoveryKey")
    }
}
