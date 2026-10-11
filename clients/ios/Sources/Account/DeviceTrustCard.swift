import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceTrustCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @State private var confirmingReset = false
    @State private var enteringRecoveryKey = false
    @State private var recoveryKey = ""
    @FocusState private var keyFocused: Bool
    let trust: AccountStore.DeviceTrust

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
            content
                .animation(theme.motion.spring, value: trust)
        }
        .confirmationDialog("Start fresh on this \(ThisDevice.name)?", isPresented: $confirmingReset, titleVisibility: .visible) {
            Button("Start fresh", role: .destructive) { model.account.startFresh() }
                .accessibilityIdentifier("deviceTrust.confirmReset")
        } message: {
            Text("""
            Your other devices lose access until you approve them again from this \(ThisDevice.name). \
            Friends confirm your identity again, and you make your rooms again. \
            If you have a recovery key, cancel and use it: you keep your friends and rooms.
            """)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch trust {
        case .choose:
            VStack(spacing: 12) {
                Button {
                    model.account.requestDeviceApproval()
                } label: {
                    Text("Approve from another device").frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(.primary, size: .large))
                .accessibilityIdentifier("deviceTrust.request")
                if enteringRecoveryKey {
                    recoveryField
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                } else {
                    Button {
                        enteringRecoveryKey = true
                        keyFocused = true
                    } label: {
                        Text("Use my recovery key").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.kodosi(.secondary, size: .large))
                    .accessibilityIdentifier("deviceTrust.useRecoveryKey")
                }
                Button("I have no other device") { confirmingReset = true }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("deviceTrust.reset")
            }
            .animation(theme.motion.spring, value: enteringRecoveryKey)
        case .requesting:
            ProgressCaption(text: String(localized: "Asking your other devices"))
        case let .pendingApproval(code):
            VStack(spacing: 18) {
                Text("On a device you already use, open Settings, then Add a device, and enter this code.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                CodeTiles(code: code, identifier: "deviceTrust.code")
                ProgressCaption(text: String(localized: "Waiting for approval"))
                Button("Cancel request") { model.account.cancelDeviceApproval() }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("deviceTrust.cancelRequest")
            }
        case .recovering:
            ProgressCaption(text: String(localized: "Checking your recovery key"))
        case .resetting:
            ProgressCaption(text: String(localized: "Starting fresh"))
        case let .failed(message):
            VStack(spacing: 14) {
                ErrorNote(message: message)
                    .accessibilityIdentifier("deviceTrust.error")
                HStack(spacing: 10) {
                    Button("Sign in again") { model.account.restartSignIn() }
                        .buttonStyle(.kodosi(.secondary))
                        .accessibilityIdentifier("deviceTrust.signInAgain")
                    Button("Try again") { model.account.chooseTrust() }
                        .buttonStyle(.kodosi(.primary))
                        .accessibilityIdentifier("deviceTrust.retry")
                }
            }
        }
    }

    private var recoveryField: some View {
        HStack(spacing: 8) {
            TextField("Recovery key", text: $recoveryKey)
                .textInputAutocapitalization(.characters)
                .autocorrectionDisabled()
                .focused($keyFocused)
                .submitLabel(.go)
                .onSubmit(recover)
                .font(.system(.callout, design: .monospaced))
                .accessibilityIdentifier("deviceTrust.recoveryKey")
            Button("Approve", action: recover)
                .buttonStyle(.kodosi(.primary))
                .disabled(recoveryKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityIdentifier("deviceTrust.recover")
        }
        .padding(.leading, 18)
        .padding(.trailing, 4)
        .frame(minHeight: 52)
        .raisedCapsule(elevation: keyFocused ? .lifted : .resting)
        .overlay {
            Capsule().strokeBorder(theme.colors.accent.opacity(keyFocused ? 0.5 : 0), lineWidth: 1.5)
        }
        .animation(theme.motion.hover, value: keyFocused)
    }

    private func recover() {
        model.account.useRecoveryKey(recoveryKey)
        recoveryKey = ""
    }
}
