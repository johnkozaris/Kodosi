import SwiftUI

struct DeviceTrustView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var confirmingReset = false
    let trust: AppDependencies.DeviceTrust

    var body: some View {
        VStack(spacing: 14) {
            switch trust {
            case .choose:
                Text("Approve this Mac from a computer you already use.")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
                Button("Approve from another device") { deps.requestDeviceApproval() }
                    .buttonStyle(.kodosi(.primary, size: .large))
                    .accessibilityIdentifier("deviceTrust.request")
                Button("I have no other device") { confirmingReset = true }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityIdentifier("deviceTrust.reset")
            case .requesting:
                ProgressCaption(text: String(localized: "Asking your other devices")).accessibilityLabel(Text("Requesting approval"))
            case let .pendingApproval(code):
                Text("On your other computer, open Settings, then Devices, and enter this code.")
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
                VerificationCode(code: code, identifier: "deviceTrust.code")
                ProgressCaption(text: String(localized: "Waiting for approval")).accessibilityLabel(Text("Waiting for approval"))
                Button("Cancel request") { deps.cancelDeviceApproval() }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityIdentifier("deviceTrust.cancelRequest")
            case .resetting:
                ProgressCaption(text: String(localized: "Starting fresh")).accessibilityLabel(Text("Resetting device identity"))
            case let .failed(message):
                ErrorNote(message: message).accessibilityIdentifier("deviceTrust.error")
                HStack(spacing: 8) {
                    Button("Sign in again") { deps.restartSignIn() }
                        .buttonStyle(.kodosi(.secondary))
                        .accessibilityIdentifier("deviceTrust.signInAgain")
                    Button("Try again") { deps.retryDeviceTrust() }
                        .buttonStyle(.kodosi(.primary))
                        .accessibilityIdentifier("deviceTrust.retry")
                }
            }
        }
        .frame(maxWidth: .infinity)
        .confirmationDialog("Start fresh on this Mac?", isPresented: $confirmingReset, titleVisibility: .visible) {
            Button("Start fresh", role: .destructive) { deps.resetTrustedDevices() }
        } message: {
            Text(
                "Your other devices lose access until you approve them again from this Mac. "
                    + "Friends confirm your identity again. Anything you shared stays shared."
            )
        }
    }
}
