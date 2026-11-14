import KodosiBridge
import KodosiDesign
import SwiftUI

enum AppStore {
    static let page = URL(string: "https://apps.apple.com/app/id6834757262")!
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.openURL) private var openURL

    private var screen: Screen {
        switch model.phase {
        case .launching: .launching
        case let .failed(message): .failed(message)
        case .ready:
            if model.account.isReady {
                .main
            } else if model.account.restoring == "updateRequired" {
                .updateRequired
            } else if model.account.needsOnboarding {
                .onboarding
            } else {
                .launching
            }
        }
    }

    var body: some View {
        ZStack {
            GroundBackdrop()
            switch screen {
            case .launching:
                LaunchView().transition(.opacity)
            case let .failed(message):
                StartFailureView(message: message, retry: model.retry).transition(.opacity)
            case .updateRequired:
                UpdateRequiredView().transition(.opacity)
            case .onboarding:
                OnboardingView().transition(.opacity.combined(with: .scale(scale: 0.98)))
            case .main:
                MainTabView().transition(.opacity)
            }
        }
        .animation(theme.motion.soft, value: screen)
        .overlay(alignment: .top) {
            if screen == .main || screen == .onboarding, model.updateRequired, !model.updateNoticeDismissed {
                NoticePill(message: String(localized: "Update Kodosi to connect."), identifier: "notice.update",
                           action: { openURL(AppStore.page) }, actionTitle: Text("Update"),
                           dismiss: { model.updateNoticeDismissed = true })
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .transition(.move(edge: .top).combined(with: .opacity))
            } else {
                NoticeBanner()
            }
        }
        .background {
            LockWindow(covers: screen == .main && model.lock.covers(scenePhase), locked: model.lock.locked) {
                LockCover().environment(model).kodosiTheme()
            }
        }
        .onChange(of: model.lock.locked) { _, locked in
            if !locked {
                model.projects.markShownRead()
            }
        }
    }

    private enum Screen: Equatable {
        case launching
        case failed(String)
        case updateRequired
        case onboarding
        case main
    }
}

struct LaunchView: View {
    var body: some View {
        KodosiWordmark(size: 26)
            .accessibilityIdentifier("launch.wordmark")
    }
}

struct UpdateRequiredView: View {
    @Environment(\.theme) private var theme

    var body: some View {
        VStack(spacing: 20) {
            IconTile(symbol: "arrow.down.circle.fill", tint: theme.colors.accent, size: 56)
            VStack(spacing: 8) {
                Text("Signing in again")
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text("Update Kodosi to sign in.")
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            Link(destination: AppStore.page) { Text("Update") }
                .buttonStyle(.kodosi(.primary, size: .large))
                .accessibilityIdentifier("launch.updateRequired.update")
        }
        .padding(32)
        .frame(maxWidth: 420)
        .accessibilityIdentifier("launch.updateRequired")
    }
}

struct StartFailureView: View {
    @Environment(\.theme) private var theme
    let message: String
    let retry: () -> Void

    var body: some View {
        VStack(spacing: 20) {
            IconTile(symbol: "exclamationmark.triangle.fill", tint: theme.colors.caution, size: 56)
            VStack(spacing: 8) {
                Text("Kodosi stopped")
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                Text(message)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            Button("Start again", action: retry)
                .buttonStyle(.kodosi(.primary, size: .large))
                .accessibilityIdentifier("launch.retry")
        }
        .padding(32)
        .frame(maxWidth: 420)
    }
}

struct NoticeBanner: View {
    @Environment(NoticeCenter.self) private var notices
    @Environment(\.theme) private var theme

    var body: some View {
        ZStack {
            if let notice = notices.current {
                NoticePill(message: notice.message, identifier: "notice", sign: notice.done ? .status(.done) : .caution) {
                    notices.dismiss(notice)
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .transition(.move(edge: .top).combined(with: .opacity))
                .task(id: notice.id) {
                    if notice.done {
                        AccessibilityNotification.Announcement(notice.message).post()
                    }
                    try? await Task.sleep(for: .seconds(6))
                    notices.dismiss(notice)
                }
                .sensoryFeedback(notice.done ? .success : .warning, trigger: notice.id)
            }
        }
        .animation(theme.motion.spring, value: notices.current)
    }
}
