import SwiftUI

struct SignInSheet: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 24) {
            HStack {
                Text("Sign in to Kodosi").appTextStyle(.headingSection)
                Spacer()
                SessionIconButton(title: "Close sign-in", symbol: "xmark", identifier: "signIn.close") { deps.dismissSignIn() }
            }
            HStack(spacing: 0) {
                connectionNode("person.crop.circle", reached: deps.signInStage.completedSteps >= 1)
                connectionLine(reached: deps.signInStage.completedSteps >= 2)
                connectionNode("globe", reached: deps.signInStage.completedSteps >= 2)
                connectionLine(reached: deps.signInStage.completedSteps >= 3)
                connectionNode(deps.accountReady ? "checkmark.circle.fill" : "laptopcomputer", reached: deps.signInStage.completedSteps >= 3)
            }.padding(.horizontal, 34).padding(.vertical, 12)
                .animation(reduceMotion ? nil : theme.motion.selection, value: deps.signInStage.completedSteps)
                .accessibilityElement(children: .ignore).accessibilityLabel(Text("Sign-in progress"))
                .accessibilityValue(Text("\(deps.signInStage.completedSteps) of 3 steps complete"))
            Group {
                switch deps.signInStage {
                case let .awaitingApproval(code, url):
                    VStack(spacing: 16) {
                        VerificationCode(code: code, identifier: "signIn.code")
                        if url != nil {
                            Button { deps.openSignInPage() } label: { Label("Open browser", systemImage: "arrow.up.right") }
                                .buttonStyle(SolidPrimaryButtonStyle()).accessibilityIdentifier("signIn.open")
                        }
                    }
                case let .trustingDevice(trust): DeviceTrustView(trust: trust)
                case let .failed(message, _): ErrorNote(message: message).accessibilityIdentifier("signIn.error")
                case .signedIn: Text("Connected").font(.system(size: 18, weight: .medium)).foregroundStyle(theme.colors.tertiary)
                case .idle, .starting, .finalizing: ProgressView().accessibilityLabel(Text("Connecting account"))
                }
            }.frame(maxWidth: .infinity, minHeight: 90)
            HStack(spacing: 12) {
                Spacer()
                switch deps.signInStage {
                case .failed:
                    Button("Close") { deps.dismissSignIn() }.accessibilityIdentifier("signIn.cancel")
                    Button("Try again") { deps.restartSignIn() }.buttonStyle(SolidPrimaryButtonStyle()).accessibilityIdentifier("signIn.retry")
                case .signedIn:
                    Button("Done") { deps.dismissSignIn() }.buttonStyle(SolidPrimaryButtonStyle()).keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("signIn.done")
                case .trustingDevice:
                    Button("Not now") { deps.dismissSignIn() }.keyboardShortcut(.cancelAction).accessibilityIdentifier("signIn.later")
                case .idle, .starting, .awaitingApproval, .finalizing:
                    Button("Cancel") { deps.cancelSignIn() }.keyboardShortcut(.cancelAction).accessibilityIdentifier("signIn.cancel")
                }
            }
        }.padding(24).frame(width: 440).background(theme.colors.background)
            .onChange(of: deps.signInStage.completedSteps) { _, steps in
                AccessibilityNotification.Announcement(String(localized: "Sign-in step \(steps) complete")).post()
            }
    }

    private func connectionNode(_ symbol: String, reached: Bool) -> some View {
        Image(systemName: symbol).font(.system(size: 30, weight: .light)).frame(width: 56, height: 56)
            .foregroundStyle(reached ? theme.colors.primary : theme.colors.mutedForeground)
            .contentTransition(.symbolEffect(.replace))
    }

    private func connectionLine(reached: Bool) -> some View {
        Capsule().fill(reached ? theme.colors.primary : theme.colors.seam).frame(height: 2)
    }
}
