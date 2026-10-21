import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceTrustView: View {
    @Environment(AppDependencies.self) private var deps
    let trust: DeviceTrust

    var body: some View {
        DeviceTrustChoices(trust: trust, device: String(localized: "Mac"), actions: .init(
            request: deps.account.requestDeviceApproval, cancelRequest: deps.account.cancelDeviceApproval, recover: deps.account.useRecoveryKey,
            startFresh: deps.account.startFresh, signInAgain: deps.restartSignIn, retry: deps.account.chooseTrust,
        ))
        .frame(maxWidth: .infinity)
    }
}
