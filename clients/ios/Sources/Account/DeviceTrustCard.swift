import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceTrustCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @State private var scanning = false
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
                Text("Kodosi encrypts your terminals and projects end to end. A device you already trust gives this \(ThisDevice.name) its keys.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } trail: {
            let account = model.account
            DeviceTrustChoices(trust: trust, device: ThisDevice.name, actions: .init(
                scan: { scanning = true },
                request: account.requestDeviceApproval, cancelRequest: account.cancelDeviceApproval,
                recover: account.useRecoveryKey, startFresh: account.startFresh, stopStartingFresh: account.stopReset,
                signInAgain: account.restartSignIn, retry: account.chooseTrust,
            ))
        }
        .sheet(isPresented: $scanning) {
            CodeScanSheet(title: "Scan the code", scan: CodeScan(
                hint: "On your computer, open Kodosi Settings. Under Add a device, choose Show a code. Then point the camera at the code.",
                other: "This is not a code from Add a device.",
                identifier: "deviceTrust.scan", accepts: { DeviceCode.scanned(in: $0, for: .join) },
            ) { code in
                scanning = false
                model.account.joinDevice(scanned: code)
            })
        }
    }
}
