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
            HStack(spacing: 0) {
                node("person.fill", label: "You", reached: steps >= 1, active: steps == 0 && deps.signInStage.isPending)
                line(0)
                node("lock.shield.fill", label: "Kodosi", reached: steps >= 2, active: steps == 1)
                line(1)
                node(deps.accountReady ? "checkmark" : "laptopcomputer", label: "This Mac", reached: steps >= 3, active: steps == 2)
            }
            .padding(.horizontal, 18)
            .animation(theme.motion.spring, value: steps)
            .accessibilityElement(children: .ignore).accessibilityLabel(Text("Sign-in progress"))
            .accessibilityValue(Text("\(steps) of 3 steps complete"))
            Group {
                switch deps.signInStage {
                case let .awaitingApproval(code, url):
                    VStack(spacing: 16) {
                        Text("Check that your browser shows this code.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        VerificationCode(code: code, identifier: "signIn.code")
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
                case .idle, .starting, .awaitingApproval, .finalizing:
                    Button("Cancel") { deps.cancelSignIn() }.buttonStyle(.kodosi(.ghost)).keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("signIn.cancel")
                }
            }
        }
        .padding(24).frame(width: 460)
        .animation(theme.motion.spring, value: deps.signInStage)
        .onChange(of: steps) { _, steps in
            AccessibilityNotification.Announcement(String(localized: "Sign-in step \(steps) complete")).post()
        }
    }

    private func node(_ symbol: String, label: LocalizedStringKey, reached: Bool, active: Bool) -> some View {
        VStack(spacing: 8) {
            Image(systemName: symbol).font(.system(size: 18, weight: .semibold))
                .contentTransition(.symbolEffect(.replace))
                .foregroundStyle(reached ? theme.colors.onAccent : theme.colors.inkFaint)
                .frame(width: 48, height: 48)
                .background {
                    if reached {
                        RaisedBackground(shape: Circle(), fill: theme.colors.accent, elevation: .resting)
                    } else {
                        WellBackground(shape: Circle())
                    }
                }
                .overlay {
                    if active {
                        CellRim(.working, lineWidth: 2.5).padding(-6).transition(.opacity)
                    }
                }
            Text(label).appTextStyle(.caption).foregroundStyle(reached || active ? theme.colors.ink : theme.colors.inkFaint)
        }
        .frame(width: 70)
    }

    /// The line after step `index`: it runs while the next step works, and it is lit once that step is done.
    private func line(_ index: Int) -> some View {
        CellLine(steps >= index + 2 ? .done : steps == index + 1 ? .flowing : .waiting)
            .padding(.bottom, 24).padding(.horizontal, -6)
    }
}
