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
                line(reached: steps >= 2)
                node("lock.shield.fill", label: "Kodosi", reached: steps >= 2, active: steps == 1)
                line(reached: steps >= 3)
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
        .padding(24).frame(width: 460).background(theme.colors.raised.mix(with: theme.colors.surface, by: 0.4))
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
                        WorkingRim(shape: Circle(), lineWidth: 2, glow: 5).padding(-2)
                    }
                }
            Text(label).appTextStyle(.caption).foregroundStyle(reached || active ? theme.colors.ink : theme.colors.inkFaint)
        }
        .frame(width: 70)
    }

    private func line(reached: Bool) -> some View {
        Capsule().fill(theme.colors.ink.opacity(0.1))
            .frame(height: 3)
            .overlay(alignment: .leading) {
                GeometryReader { geometry in
                    Capsule().fill(theme.colors.accent).frame(width: reached ? geometry.size.width : 0)
                }
            }
            .padding(.bottom, 24).padding(.horizontal, -8)
    }
}
