import KodosiBridge
import KodosiDesign
import SwiftUI

struct OnboardingView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.verticalSizeClass) private var verticalSizeClass

    private var account: AccountStore {
        model.account
    }

    private var compactHeight: Bool {
        verticalSizeClass == .compact
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

    private var showsTrack: Bool {
        switch account.stage {
        case .awaitingApproval, .finalizing, .trustingDevice: true
        case .idle, .starting, .signedIn, .failed: false
        }
    }

    private var failed: Bool {
        switch account.stage {
        case .failed, .trustingDevice(.failed): true
        default: false
        }
    }

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                VStack(spacing: compactHeight ? 20 : 32) {
                    if !(compactHeight && showsTrack) {
                        KodosiWordmark(size: compactHeight ? 22 : 30)
                            .padding(.top, compactHeight ? 0 : 12)
                            .transition(.opacity)
                    }
                    if showsTrack {
                        SignInTrack(
                            completed: account.stage.completedSteps, working: account.stage.isPending,
                            device: ThisDevice.title, symbol: ThisDevice.symbol
                        )
                        .transition(.opacity.combined(with: .offset(y: -10)))
                    }
                    stageContent
                        .frame(maxWidth: compactHeight ? 760 : 440)
                        .id(stageKey)
                        .transition(.asymmetric(
                            insertion: .opacity.combined(with: .offset(y: 14)),
                            removal: .opacity
                        ))
                }
                .padding(.horizontal, 24)
                .padding(.vertical, compactHeight ? 16 : 32)
                .frame(maxWidth: .infinity, minHeight: geometry.size.height)
            }
            .scrollBounceBehavior(.basedOnSize)
            .scrollDismissesKeyboard(.interactively)
        }
        .animation(theme.motion.soft, value: stageKey)
        .animation(theme.motion.soft, value: showsTrack)
        .sensoryFeedback(.success, trigger: account.stage == .signedIn) { _, signedIn in signedIn }
        .sensoryFeedback(.error, trigger: failed) { _, failed in failed }
        .sheet(isPresented: safariPresented) {
            if let url = account.signInPage {
                SafariView(url: url).ignoresSafeArea()
            }
        }
    }

    private var stageKey: String {
        switch account.stage {
        case .idle, .failed, .signedIn, .starting: "welcome"
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
        case let .failed(message, _):
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

struct StageColumns<Lead: View, Trail: View>: View {
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    var spacing: CGFloat = 24
    @ViewBuilder let lead: () -> Lead
    @ViewBuilder let trail: () -> Trail

    var body: some View {
        let layout = verticalSizeClass == .compact
            ? AnyLayout(HStackLayout(alignment: .center, spacing: 40))
            : AnyLayout(VStackLayout(spacing: spacing))
        layout {
            lead().frame(maxWidth: .infinity)
            trail().frame(maxWidth: .infinity)
        }
    }
}

private struct WelcomeCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    let failure: String?
    let starting: Bool

    var body: some View {
        if verticalSizeClass == .compact {
            HStack(alignment: .center, spacing: 40) {
                VStack(spacing: 22) {
                    intro
                    actions
                }
                .frame(maxWidth: .infinity)
                FeatureStrip()
                    .frame(maxWidth: .infinity)
            }
        } else {
            VStack(spacing: 28) {
                intro
                FeatureStrip()
                actions
            }
        }
    }

    private var intro: some View {
        VStack(spacing: 12) {
            Text("Your terminals, with you")
                .appTextStyle(.large)
                .foregroundStyle(theme.colors.ink)
                .multilineTextAlignment(.center)
            Text("See what your agents do, answer them, and talk with your rooms. Your terminals keep running on your computers.")
                .appTextStyle(.callout)
                .foregroundStyle(theme.colors.inkMuted)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .arrive()
    }

    private var actions: some View {
        VStack(spacing: 12) {
            if let failure {
                ErrorNote(message: failure)
                    .accessibilityIdentifier("signIn.error")
                    .transition(AnyTransition.rise)
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
            HStack(spacing: 5) {
                Image(systemName: "lock.fill")
                    .imageScale(.small)
                Text("Encrypted end to end")
            }
            .appTextStyle(.caption)
            .foregroundStyle(theme.colors.inkFaint)
        }
        .arrive(delay: 0.2)
    }
}

private struct FeatureStrip: View {
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            FeatureRow(title: "Follow your agents", detail: "Live terminals from every computer you use.") {
                AgentMark(kind: .claude, size: 34, activity: .working)
            }
            .arrive(delay: 0.06)
            FeatureRow(title: "Answer when they ask", detail: "Approvals and questions come to you first.") {
                AgentMark(kind: .codex, size: 34)
                    .overlay(alignment: .topTrailing) {
                        StatusSign(form: .hand, size: 11, backing: theme.colors.raised).offset(x: 6, y: -6)
                    }
            }
            .arrive(delay: 0.1)
            FeatureRow(title: "Work in rooms", detail: "Talk, share terminals, and keep tasks with your team.") {
                RoomSigil(key: "kodosi-room", size: 34)
            }
            .arrive(delay: 0.14)
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .raised(Radius.xl)
    }
}

private struct FeatureRow<Mark: View>: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    let detail: LocalizedStringKey
    @ViewBuilder let mark: () -> Mark

    var body: some View {
        HStack(spacing: 14) {
            mark()
            VStack(alignment: .leading, spacing: 2) {
                Text(title).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                Text(detail)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
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
        StageColumns {
            VStack(spacing: 10) {
                Text("Confirm this code")
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("The sign-in page shows the same code. Confirm it there, then come back.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } trail: {
            VStack(spacing: 22) {
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
}
