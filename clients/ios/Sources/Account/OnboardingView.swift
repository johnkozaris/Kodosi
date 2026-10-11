import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct OnboardingView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme

    private var account: AccountStore {
        model.account
    }

    private var safariPresented: Binding<Bool> {
        Binding(
            get: { account.signInPage != nil },
            set: { presented in
                if !presented {
                    account.closeSignInPage()
                }
            }
        )
    }

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                VStack(spacing: 36) {
                    KodosiWordmark(size: 30)
                        .padding(.top, 12)
                    stageContent
                        .frame(maxWidth: 440)
                        .id(stageKey)
                        .transition(.asymmetric(
                            insertion: .opacity.combined(with: .offset(y: 14)),
                            removal: .opacity
                        ))
                }
                .padding(.horizontal, 24)
                .padding(.vertical, 32)
                .frame(maxWidth: .infinity, minHeight: geometry.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .animation(theme.motion.soft, value: stageKey)
        .sensoryFeedback(.success, trigger: account.stage == .signedIn)
        .sheet(isPresented: safariPresented) {
            if let url = account.signInPage {
                SafariView(url: url).ignoresSafeArea()
            }
        }
    }

    private var stageKey: String {
        switch account.stage {
        case .idle, .failed, .signedIn: "welcome"
        case .starting: "welcome"
        case .awaitingApproval: "code"
        case .finalizing: "finalizing"
        case .trustingDevice: "trust"
        }
    }

    @ViewBuilder
    private var stageContent: some View {
        switch account.stage {
        case .idle, .signedIn:
            WelcomeCard(failure: nil, starting: false)
        case let .failed(message):
            WelcomeCard(failure: message, starting: false)
        case .starting:
            WelcomeCard(failure: nil, starting: true)
        case let .awaitingApproval(code, url):
            ApprovalCodeCard(code: code, hasPage: url != nil)
        case .finalizing:
            ProgressCaption(text: String(localized: "Signing in"))
                .accessibilityIdentifier("signIn.finalizing")
        case let .trustingDevice(trust):
            DeviceTrustCard(trust: trust)
        }
    }
}

private struct WelcomeCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let failure: String?
    let starting: Bool

    var body: some View {
        VStack(spacing: 28) {
            VStack(spacing: 12) {
                Text("Your terminals, with you")
                    .appTextStyle(.large)
                    .foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.center)
                Text("See what your agents do, answer them, and talk with your rooms. Your terminals keep running on your computers.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            .arrive()
            FeatureStrip()
                .arrive(delay: 0.08)
            VStack(spacing: 12) {
                if let failure {
                    ErrorNote(message: failure)
                        .accessibilityIdentifier("signIn.error")
                }
                Button {
                    model.account.signIn()
                } label: {
                    HStack(spacing: 10) {
                        if starting {
                            ProgressView().tint(theme.colors.onAccent)
                        }
                        Text(starting ? "Opening sign-in" : "Sign in")
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(.primary, size: .large))
                .disabled(starting)
                .accessibilityIdentifier("signIn.start")
            }
            .arrive(delay: 0.16)
        }
    }
}

private struct FeatureStrip: View {
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            row(mark: AnyView(AgentMark(kind: .claude, size: 34, activity: .working)),
                title: "Follow your agents",
                detail: "Live terminals from every computer you use.")
            row(mark: AnyView(AgentMark(kind: .codex, size: 34, activity: .asks)),
                title: "Answer when they ask",
                detail: "Approvals and questions come to you first.")
            row(mark: AnyView(RoomSigil(name: "Room", key: "kodosi-room", size: 34)),
                title: "Work in rooms",
                detail: "Talk, share terminals, and keep tasks with your team.")
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.xl)
    }

    private func row(mark: AnyView, title: LocalizedStringKey, detail: LocalizedStringKey) -> some View {
        HStack(spacing: 14) {
            mark
            VStack(alignment: .leading, spacing: 2) {
                Text(title).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text(detail).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

private struct ApprovalCodeCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let code: String
    let hasPage: Bool

    var body: some View {
        VStack(spacing: 24) {
            VStack(spacing: 10) {
                Text("Confirm this code")
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("The sign-in page shows the same code. Confirm it there, then come back.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            VerificationCode(code: code, identifier: "signIn.code")
            ProgressCaption(text: String(localized: "Waiting for the sign-in page"))
            VStack(spacing: 10) {
                if hasPage {
                    Button {
                        model.account.reopenSignInPage()
                    } label: {
                        Text("Open sign-in page").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.kodosi(.primary, size: .large))
                    .accessibilityIdentifier("signIn.openPage")
                }
                Button("Cancel") { model.account.cancelSignIn() }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("signIn.cancel")
            }
        }
    }
}

private struct DeviceTrustCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingReset = false
    @State private var enteringRecoveryKey = false
    @State private var recoveryKey = ""
    @FocusState private var keyFocused: Bool
    let trust: AccountStore.DeviceTrust

    private var deviceName: String {
        UIDevice.current.userInterfaceIdiom == .pad ? String(localized: "iPad") : String(localized: "iPhone")
    }

    var body: some View {
        VStack(spacing: 24) {
            VStack(spacing: 14) {
                IconTile(symbol: UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone", size: 56)
                Text("Trust this \(deviceName)")
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("Kodosi encrypts your terminals and rooms end to end. A device you already trust gives this \(deviceName) its keys.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            content
        }
        .confirmationDialog("Start fresh on this \(deviceName)?", isPresented: $confirmingReset, titleVisibility: .visible) {
            Button("Start fresh", role: .destructive) { model.account.startFresh() }
                .accessibilityIdentifier("deviceTrust.confirmReset")
        } message: {
            Text(
                "Your other devices lose access until you approve them again from this \(deviceName). "
                    + "Friends confirm your identity again, and you make your rooms again. "
                    + "If you have a recovery key, cancel and use it: you keep your friends and rooms."
            )
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
                Text("On your computer, open Settings, then Devices, and enter this code.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                VerificationCode(code: code, identifier: "deviceTrust.code")
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
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .focused($keyFocused)
                .submitLabel(.go)
                .onSubmit(recover)
                .appTextStyle(.callout)
                .accessibilityIdentifier("deviceTrust.recoveryKey")
            Button("Approve", action: recover)
                .buttonStyle(.kodosi(.primary))
                .disabled(recoveryKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityIdentifier("deviceTrust.recover")
        }
        .padding(.leading, 18)
        .padding(.trailing, 4)
        .frame(height: 52)
        .raisedCapsule()
    }

    private func recover() {
        model.account.useRecoveryKey(recoveryKey)
        recoveryKey = ""
    }
}
