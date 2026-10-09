import AppKit
import SwiftUI

struct KodosiDesktopApp: App {
    @NSApplicationDelegateAdaptor private var appDelegate: AppDelegate

    var body: some Scene {
        Window("Kodosi", id: "main") {
            RootView()
                .environment(appDelegate.dependencies)
                .kodosiTheme()
                .defaultAppStorage(appDelegate.storageBootstrap.defaults)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1280, height: 820)
        .commands { KodosiCommands(dependencies: appDelegate.dependencies) }
    }
}

struct RootView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        Group {
            switch deps.appState {
            case .launching:
                VStack(spacing: 18) {
                    KodosiWordmark(size: 30)
                    Text("Starting").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).shimmer()
                }
                .accessibilityElement(children: .ignore).accessibilityLabel(Text("Starting Kodosi…"))
            case .ready:
                AppShell()
            case let .error(message):
                EmptyState(title: "Kodosi could not start") {
                    KodosiWordmark(size: 24, blinks: false)
                } actions: {
                    VStack(spacing: 16) {
                        Text(message).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .multilineTextAlignment(.center).frame(maxWidth: 460).textSelection(.enabled)
                        Button("Start again") { deps.retryStartup() }
                            .buttonStyle(.kodosi(.primary, size: .large))
                            .accessibilityIdentifier("root.error.retry")
                    }
                }
            case let .hostConflict(failure):
                EmptyState(title: "Kodosi is already running") {
                    KodosiWordmark(size: 24, blinks: false)
                } actions: {
                    VStack(spacing: 16) {
                        Text(Self.describe(failure)).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .multilineTextAlignment(.center).frame(maxWidth: 460)
                        if let notice = deps.errorMessage {
                            ErrorNote(message: notice).frame(maxWidth: 460)
                        }
                        HStack(spacing: 8) {
                            Button("Try again") { deps.retryStartup() }
                                .buttonStyle(.kodosi(.secondary, size: .large))
                                .accessibilityIdentifier("root.hostConflict.retry")
                            if failure.resolution == .quitDuplicateApp {
                                Button("Quit") { NSApplication.shared.terminate(nil) }
                                    .buttonStyle(.kodosi(.primary, size: .large))
                                    .accessibilityIdentifier("root.hostConflict.quit")
                            } else {
                                Button(failure.hostLocalSessions == 0
                                    ? String(localized: "Stop the other one")
                                    : String(localized: "Stop it and its terminals")) { deps.stopConflictingHost() }
                                    .buttonStyle(.kodosi(.primary, size: .large))
                                    .accessibilityIdentifier("root.hostConflict.stop")
                            }
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background { GroundBackdrop() }
    }

    private static func describe(_ failure: RuntimeStartFailure) -> String {
        let terminals = String(failure.hostLocalSessions)
        let pid = String(failure.hostPID)
        switch failure.hostKind {
        case .app:
            return String(localized: "Kodosi is open in another window. Use that one, or quit this one.")
        case .foreground:
            return String(localized: """
            The `kodosi host` command (process \(pid)) runs \(terminals) terminals. \
            Stop it in its terminal, or stop it here.
            """)
        case .background:
            return String(localized: """
            The kodosi command (process \(pid)) runs \(terminals) terminals in the background. \
            If you stop it, they end.
            """)
        case .unknown:
            return failure.message
        }
    }
}
