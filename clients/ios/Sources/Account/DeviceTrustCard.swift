import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceTrustCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    let trust: DeviceTrust

    var body: some View {
        StageColumns {
            VStack(spacing: 14) {
                if verticalSizeClass != .compact {
                    IconTile(symbol: ThisDevice.symbol, size: 56)
                }
                Text("Trust this \(ThisDevice.name)")
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("Kodosi encrypts your terminals and rooms end to end. A device you already trust gives this \(ThisDevice.name) its keys.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } trail: {
            let account = model.account
            DeviceTrustChoices(trust: trust, device: ThisDevice.name, actions: .init(
                request: account.requestDeviceApproval, cancelRequest: account.cancelDeviceApproval,
                recover: account.useRecoveryKey, startFresh: account.startFresh,
                signInAgain: account.restartSignIn, retry: account.chooseTrust,
            ))
        }
    }
}
