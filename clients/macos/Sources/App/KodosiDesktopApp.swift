import AppKit
import KodosiBridge
import KodosiDesign
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
        .defaultLaunchBehavior(.presented)
        .commands { KodosiCommands(dependencies: appDelegate.dependencies) }
        MenuBarExtra(isInserted: .constant(appDelegate.dependencies.settings.keepRunning)) {
            MenuBarMenu()
        } label: {
            Image("MenuBarMark").accessibilityLabel(Text("Kodosi"))
        }
    }
}

private struct MenuBarMenu: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Button("Open Kodosi") {
            openWindow(id: "main")
            MainWindowLocator.activate()
        }
        Divider()
        Button("Quit Kodosi") { NSApp.terminate(nil) }
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
            case let .hostOutdated(failure):
                EmptyState(title: "Kodosi was updated") {
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
                                .accessibilityIdentifier("root.hostOutdated.retry")
                            if failure.hostKind != .foreground {
                                Button(failure.hostTerminals == 0
                                    ? String(localized: "Restart Kodosi")
                                    : String(localized: "Restart it and end its terminals")) { deps.restartOutdatedHost() }
                                    .buttonStyle(.kodosi(.primary, size: .large))
                                    .accessibilityIdentifier("root.hostOutdated.restart")
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
        let terminals = String(failure.hostTerminals)
        let pid = String(failure.hostPID)
        switch failure.hostKind {
        case .foreground:
            return String(localized: """
            The `kodosi host` command (process \(pid)) runs the earlier version. \
            Stop it in its terminal and start it again.
            """)
        case .background where failure.hostTerminals == 1:
            return String(localized: """
            The earlier version of Kodosi still runs 1 terminal in the background. \
            Restart it to use the update. The terminal ends.
            """)
        case .background:
            return String(localized: """
            The earlier version of Kodosi still runs \(terminals) terminals in the background. \
            Restart it to use the update. Its terminals end.
            """)
        case .unknown:
            return failure.message
        }
    }
}
