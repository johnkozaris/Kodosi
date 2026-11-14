import KodosiBridge
import SwiftUI

public struct DeviceTrustChoices: View {
    public struct Actions {
        let scan: (() -> Void)?
        let request: () -> Void
        let cancelRequest: () -> Void
        let recover: (String) -> Void
        let startFresh: () -> Void
        let stopStartingFresh: () -> Void
        let signInAgain: () -> Void
        let retry: () -> Void

        public init(
            scan: (() -> Void)? = nil,
            request: @escaping () -> Void, cancelRequest: @escaping () -> Void, recover: @escaping (String) -> Void,
            startFresh: @escaping () -> Void, stopStartingFresh: @escaping () -> Void,
            signInAgain: @escaping () -> Void, retry: @escaping () -> Void,
        ) {
            self.scan = scan
            self.request = request
            self.cancelRequest = cancelRequest
            self.recover = recover
            self.startFresh = startFresh
            self.stopStartingFresh = stopStartingFresh
            self.signInAgain = signInAgain
            self.retry = retry
        }
    }

    @Environment(\.theme) private var theme
    @State private var confirmingReset = false
    @State private var enteringRecoveryKey = false
    @State private var recoveryKey = ""
    @FocusState private var keyFocused: Bool
    let trust: DeviceTrust
    let device: String
    let actions: Actions

    public init(trust: DeviceTrust, device: String, actions: Actions) {
        self.trust = trust
        self.device = device
        self.actions = actions
    }

    public var body: some View {
        content
            .animation(theme.motion.spring, value: trust)
            .confirmationDialog(
                Text("Start fresh on this \(device)?", bundle: .module), isPresented: $confirmingReset, titleVisibility: .visible,
            ) {
                Button(role: .destructive, action: actions.startFresh) { Text("Start fresh", bundle: .module) }
                    .accessibilityIdentifier("deviceTrust.confirmReset")
            } message: {
                Text("""
                Your other devices lose access until you approve them again from this \(device). \
                Friends confirm your identity again, and you make your projects again. \
                When your account has devices, this \(device) waits a day. \
                If you have another device or a recovery key, cancel and use it: you keep your friends and projects.
                """, bundle: .module)
            }
    }

    @ViewBuilder
    private var content: some View {
        switch trust {
        case .choose:
            VStack(spacing: 12) {
                if let scan = actions.scan {
                    Button(action: scan) {
                        Label {
                            Text("Scan the code on your computer", bundle: .module)
                        } icon: {
                            Image(systemName: "qrcode.viewfinder")
                        }
                        .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.kodosi(.primary, size: .large))
                    .accessibilityIdentifier("deviceTrust.scan")
                }
                Button(action: actions.request) {
                    Text("Approve from another device", bundle: .module).frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(actions.scan == nil ? .primary : .secondary, size: .large))
                .accessibilityIdentifier("deviceTrust.request")
                if enteringRecoveryKey {
                    recoveryField
                        .transition(.opacity.combined(with: .scale(scale: 0.96)))
                } else {
                    Button {
                        enteringRecoveryKey = true
                        keyFocused = true
                    } label: {
                        Text("Use my recovery key", bundle: .module).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.kodosi(.secondary, size: .large))
                    .accessibilityIdentifier("deviceTrust.useRecoveryKey")
                }
                Button { confirmingReset = true } label: { Text("I have no other device", bundle: .module) }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("deviceTrust.reset")
            }
            .animation(theme.motion.spring, value: enteringRecoveryKey)
        case .requesting:
            ProgressCaption(text: String(localized: "Asking your other devices", bundle: .module))
        case let .pendingApproval(code, qr):
            VStack(spacing: 18) {
                if qr.isEmpty {
                    typedCodeSteps
                } else {
                    ViewThatFits(in: .vertical) {
                        scannedCode(qr, side: 200)
                        scannedCode(qr, side: 132)
                        typedCodeSteps
                    }
                }
                VerificationCode(code: code, identifier: "deviceTrust.code")
                ProgressCaption(text: String(localized: "Waiting for approval", bundle: .module))
                Button(action: actions.cancelRequest) { Text("Cancel request", bundle: .module) }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("deviceTrust.cancelRequest")
            }
        case .joining:
            VStack(spacing: 14) {
                ProgressCaption(text: String(localized: "Waiting for your computer", bundle: .module))
                Button(action: actions.cancelRequest) { Text("Cancel request", bundle: .module) }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("deviceTrust.cancelRequest")
            }
        case .recovering:
            ProgressCaption(text: String(localized: "Checking your recovery key", bundle: .module))
        case .resetting:
            ProgressCaption(text: String(localized: "Starting fresh", bundle: .module))
        case let .waiting(readyAt):
            VStack(spacing: 14) {
                Text("""
                This \(device) can start fresh after \(readyAt, format: .dateTime.weekday(.wide).hour().minute()). \
                Until then, your other devices can stop it. \
                If you still have one of them, approve this device with it instead, and keep your friends and projects.
                """, bundle: .module)
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("deviceTrust.waiting")
                Button(action: actions.startFresh) {
                    Text("Start fresh", bundle: .module).frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(.primary, size: .large))
                .accessibilityIdentifier("deviceTrust.startFresh")
                Button(action: actions.stopStartingFresh) { Text("Do not start fresh", bundle: .module) }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("deviceTrust.stopReset")
            }
        case let .failed(message):
            VStack(spacing: 14) {
                ErrorNote(message: message)
                    .accessibilityIdentifier("deviceTrust.error")
                HStack(spacing: 10) {
                    Button(action: actions.signInAgain) { Text("Sign in again", bundle: .module) }
                        .buttonStyle(.kodosi(.secondary))
                        .accessibilityIdentifier("deviceTrust.signInAgain")
                    Button(action: actions.retry) { Text("Try again", bundle: .module) }
                        .buttonStyle(.kodosi(.primary))
                        .accessibilityIdentifier("deviceTrust.retry")
                }
            }
        }
    }

    private var typedCodeSteps: some View {
        steps(Text("On a device you already use, open Settings and choose Add a device. Then enter this code.", bundle: .module))
    }

    private func scannedCode(_ qr: [String], side: CGFloat) -> some View {
        VStack(spacing: 18) {
            steps(Text("On a device you already use, open Settings and choose Add a device. Then scan this code, or enter it.", bundle: .module))
            ScanCodeCard {
                ScanCode(rows: qr)
                    .accessibilityElement()
                    .accessibilityLabel(Text("QR code of this code", bundle: .module))
                    .accessibilityAddTraits(.isImage)
                    .accessibilityIdentifier("deviceTrust.qr")
            }
            .frame(width: side, height: side)
            .arrive()
        }
    }

    private func steps(_ text: Text) -> some View {
        text
            .appTextStyle(.callout)
            .foregroundStyle(theme.colors.inkMuted)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
    }

    private var recoveryField: some View {
        HStack(spacing: 8) {
            TextField(text: $recoveryKey) { Text("Recovery key", bundle: .module) }
            #if os(iOS)
                .textInputAutocapitalization(.characters)
                .submitLabel(.go)
            #endif
                .textFieldStyle(.plain)
                .autocorrectionDisabled()
                .focused($keyFocused)
                .onSubmit(recover)
                .font(.system(.callout, design: .monospaced))
                .accessibilityIdentifier("deviceTrust.recoveryKey")
            Button(action: recover) { Text("Approve", bundle: .module) }
                .buttonStyle(.kodosi(.primary))
                .disabled(recoveryKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityIdentifier("deviceTrust.recover")
        }
        .padding(.leading, 18)
        .padding(.trailing, 4)
        .frame(minHeight: 52)
        .raisedCapsule(elevation: keyFocused ? .lifted : .resting)
        .animation(theme.motion.hover, value: keyFocused)
    }

    private func recover() {
        let key = recoveryKey.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        actions.recover(key)
        recoveryKey = ""
    }
}
