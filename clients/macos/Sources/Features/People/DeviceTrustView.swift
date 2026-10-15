import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceTrustView: View {
    @Environment(AppDependencies.self) private var deps
    let trust: DeviceTrust

    var body: some View {
        DeviceTrustChoices(trust: trust, device: String(localized: "Mac"), actions: .init(
            request: deps.requestDeviceApproval, cancelRequest: deps.cancelDeviceApproval, recover: deps.useRecoveryKey,
            startFresh: deps.resetTrustedDevices, signInAgain: deps.restartSignIn, retry: deps.retryDeviceTrust
        ))
        .frame(maxWidth: .infinity)
    }
}
