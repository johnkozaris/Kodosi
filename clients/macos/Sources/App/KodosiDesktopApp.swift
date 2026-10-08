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
        .windowStyle(.automatic)
        .defaultSize(width: 1200, height: 800)
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
                ProgressView("Starting Kodosi…")
            case .ready:
                AppShell()
            case let .error(message):
                VStack(spacing: 16) {
                    Text("Kodosi couldn’t start").appTextStyle(.headingSection)
                    Text(message).appTextStyle(.body).foregroundStyle(theme.colors.mutedForeground)
                        .multilineTextAlignment(.center).frame(maxWidth: 500)
                    Button("Restart runtime") { deps.retryStartup() }
                        .buttonStyle(SolidPrimaryButtonStyle())
                        .accessibilityIdentifier("root.error.retry")
                }
                .padding(24)
            case let .hostConflict(failure):
                VStack(spacing: 16) {
                    Text("Another Kodosi host is running").appTextStyle(.headingSection)
                    Text(Self.describe(failure)).appTextStyle(.body).foregroundStyle(theme.colors.mutedForeground)
                        .multilineTextAlignment(.center).frame(maxWidth: 500)
                    if let notice = deps.errorMessage {
                        Text(notice).appTextStyle(.body).foregroundStyle(theme.colors.mutedForeground)
                            .multilineTextAlignment(.center).frame(maxWidth: 500)
                    }
                    HStack(spacing: 12) {
                        if failure.resolution == .quitDuplicateApp {
                            Button("Quit") { NSApplication.shared.terminate(nil) }
                                .buttonStyle(SolidPrimaryButtonStyle())
                                .accessibilityIdentifier("root.hostConflict.quit")
                        } else {
                            Button(failure.hostLocalSessions == 0
                                ? String(localized: "Stop that host")
                                : String(localized: "Stop that host and its terminals")) { deps.stopConflictingHost() }
                                .buttonStyle(SolidPrimaryButtonStyle())
                                .accessibilityIdentifier("root.hostConflict.stop")
                        }
                        Button("Try again") { deps.retryStartup() }
                            .buttonStyle(.bordered)
                            .accessibilityIdentifier("root.hostConflict.retry")
                    }
                }
                .padding(24)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(theme.colors.background)
        .onChange(of: theme.isDark, initial: true) { _, dark in
            if case .ready = deps.appState {
                deps.commandSink.setHostTheme(dark: dark)
            }
        }
        .onChange(of: deps.appState) { _, state in
            if case .ready = state {
                deps.commandSink.setHostTheme(dark: theme.isDark)
            }
        }
    }

    private static func describe(_ failure: RuntimeStartFailure) -> String {
        let terminals = String(failure.hostLocalSessions)
        let pid = String(failure.hostPID)
        switch failure.hostKind {
        case .app:
            return String(localized: "Kodosi is already open in another window. Use that copy, or quit this one.")
        case .foreground:
            return String(localized: """
            A host started with `kodosi host` (process \(pid)) is running \(terminals) terminal(s). \
            Stop it from its terminal, or stop it here.
            """)
        case .background:
            return String(localized: """
            A background host started by the kodosi command (process \(pid)) is running \(terminals) terminal(s). \
            Stopping it ends them.
            """)
        case .unknown:
            return failure.message
        }
    }
}
