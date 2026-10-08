import SwiftUI

struct DeviceTrustView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var confirmingReset = false
    let trust: AppDependencies.DeviceTrust

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch trust {
            case .choose:
                HStack(spacing: 10) {
                    Button("Approve from another device") { deps.requestDeviceApproval() }
                        .buttonStyle(SolidPrimaryButtonStyle())
                        .accessibilityIdentifier("deviceTrust.request")
                    Button("Start fresh…") { confirmingReset = true }
                        .buttonStyle(SolidSecondaryButtonStyle())
                        .accessibilityIdentifier("deviceTrust.reset")
                }
            case .requesting:
                ProgressView().accessibilityLabel(Text("Requesting approval"))
            case let .pendingApproval(code):
                Label("Approve in Devices on your other computer", systemImage: "laptopcomputer.and.arrow.down")
                    .appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                VerificationCode(code: code, identifier: "deviceTrust.code")
                ProgressView().controlSize(.small).accessibilityLabel(Text("Waiting for approval"))
                Button("Cancel request") { deps.cancelDeviceApproval() }
                    .buttonStyle(SolidSecondaryButtonStyle())
                    .accessibilityIdentifier("deviceTrust.cancelRequest")
            case .resetting:
                ProgressView().accessibilityLabel(Text("Resetting device identity"))
            case let .failed(message):
                ErrorNote(message: message).accessibilityIdentifier("deviceTrust.error")
                HStack(spacing: 10) {
                    Button("Try again") { deps.retryDeviceTrust() }
                        .buttonStyle(SolidPrimaryButtonStyle())
                        .accessibilityIdentifier("deviceTrust.retry")
                    Button("Sign in again") { deps.restartSignIn() }
                        .buttonStyle(SolidSecondaryButtonStyle())
                        .accessibilityIdentifier("deviceTrust.signInAgain")
                }
            }
        }
        .confirmationDialog("Start fresh on this Mac?", isPresented: $confirmingReset, titleVisibility: .visible) {
            Button("Start fresh", role: .destructive) { deps.resetTrustedDevices() }
        } message: {
            Text(
                "Every other device loses access to your account and shared terminals until you approve it again from this Mac. "
                    + "Friends will be asked to confirm your identity again. Anything already shared stays shared."
            )
        }
    }
}
