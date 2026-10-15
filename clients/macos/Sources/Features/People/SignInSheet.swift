import KodosiBridge
import KodosiDesign
import SwiftUI

struct SignInSheet: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    private var steps: Int {
        deps.signInStage.completedSteps
    }

    var body: some View {
        VStack(spacing: 26) {
            HStack {
                KodosiWordmark(size: 15, blinks: deps.signInStage.isPending)
                Spacer()
                IconButton(title: "Close", symbol: "xmark", identifier: "signIn.close") { deps.dismissSignIn() }
            }
            SignInTrack(
                completed: steps, working: deps.signInStage.isPending,
                device: String(localized: "This Mac"), symbol: "laptopcomputer"
            )
            Group {
                switch deps.signInStage {
                case let .awaitingBrowser(url):
                    VStack(spacing: 16) {
                        ProgressCaption(text: String(localized: "Continue in your browser"))
                        if url != nil {
                            Button { deps.openSignInPage() } label: { Label("Open the browser again", systemImage: "arrow.up.right") }
                                .buttonStyle(.kodosi(.ghost, size: .small)).accessibilityIdentifier("signIn.open")
                        }
                    }
                case let .trustingDevice(trust): DeviceTrustView(trust: trust)
                case let .failed(message, _): ErrorNote(message: message).accessibilityIdentifier("signIn.error")
                case .signedIn:
                    VStack(spacing: 6) {
                        Text("You are in").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                        Text("Share a terminal or make a room.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                case .idle, .starting, .finalizing:
                    ProgressCaption(text: String(localized: "Connecting your account")).accessibilityLabel(Text("Connecting account"))
                }
            }
            .frame(maxWidth: .infinity, minHeight: 104)
            .transition(.opacity)
            HStack(spacing: 8) {
                Spacer()
                switch deps.signInStage {
                case .failed:
                    Button("Close") { deps.dismissSignIn() }.buttonStyle(.kodosi(.ghost)).accessibilityIdentifier("signIn.cancel")
                    Button("Try again") { deps.restartSignIn() }.buttonStyle(.kodosi(.primary)).accessibilityIdentifier("signIn.retry")
                case .signedIn:
                    Button("Done") { deps.dismissSignIn() }.buttonStyle(.kodosi(.primary)).keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("signIn.done")
                case .trustingDevice:
                    Button("Not now") { deps.dismissSignIn() }.buttonStyle(.kodosi(.ghost)).keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("signIn.later")
                case .idle, .starting, .awaitingBrowser, .finalizing:
                    Button("Cancel") { deps.cancelSignIn() }.buttonStyle(.kodosi(.ghost)).keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("signIn.cancel")
                }
            }
        }
        .padding(24).frame(width: 460)
        .animation(theme.motion.spring, value: deps.signInStage)
    }
}
