import AppKit
import os
@preconcurrency import UserNotifications

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    static let minimumWindowSize = NSSize(width: 760, height: 540)

    private static let windowChromeSettle: TimeInterval = 0.3
    private static let shutdownDeadline: TimeInterval = 12

    private var terminating = false
    private var terminationReplied = false

    let storageBootstrap: RuntimeStorageBootstrap
    let dependencies: AppDependencies

    override init() {
        do {
            #if DEBUG
                try DebugBackendConfiguration.resolve(
                    info: Bundle.main.infoDictionary ?? [:]
                )?.install()
            #endif
            let bootstrap = try RuntimeStorageBootstrap.resolveForLaunch()
            storageBootstrap = bootstrap
            dependencies = AppDependencies(
                defaults: bootstrap.defaults,
                storageBootstrap: bootstrap
            )
            super.init()
        } catch {
            fatalError("Kodosi startup storage validation failed: \(error.localizedDescription)")
        }
    }

    func applicationDidFinishLaunching(_: Notification) {
        UNUserNotificationCenter.current().delegate = self
        dependencies.start()
        let isRunning = dependencies.runtimeHandle.isRunning
        Logger.app.info("App launched. Runtime running: \(isRunning)")

        configureMainWindow()
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.windowChromeSettle) {
            self.ensureWindowOnAvailableScreen()
        }
    }

    private func configureMainWindow() {
        guard let window = NSApplication.shared.windows.first(where: { $0.isKeyWindow || $0.isMainWindow })
            ?? NSApplication.shared.windows.first else { return }
        window.backgroundColor = NSColor(red: 0.04, green: 0.035, blue: 0.025, alpha: 1)
        window.minSize = Self.minimumWindowSize
        window.isOpaque = true
    }

    private func ensureWindowOnAvailableScreen() {
        guard let mainScreen = NSScreen.main,
              let window = NSApplication.shared.windows.first(where: { $0.isVisible })
              ?? NSApplication.shared.windows.first else { return }
        let corrected = Self.correctedWindowFrame(
            window.frame,
            visibleFrames: NSScreen.screens.map(\.visibleFrame),
            mainVisibleFrame: mainScreen.visibleFrame
        )
        guard corrected != window.frame else { return }
        window.setFrame(corrected, display: true, animate: false)
        window.makeKeyAndOrderFront(nil)
    }

    func applicationShouldTerminate(_ application: NSApplication) -> NSApplication.TerminateReply {
        if terminating {
            return .terminateLater
        }
        if !storageBootstrap.isIsolated, dependencies.sessions.contains(where: { $0.kind == .local }) {
            let alert = NSAlert()
            alert.messageText = String(localized: "Quit Kodosi?")
            alert.informativeText = String(localized: "Terminals on this Mac will close.")
            alert.alertStyle = .warning
            alert.addButton(withTitle: String(localized: "Cancel"))
            alert.addButton(withTitle: String(localized: "Quit"))
            guard alert.runModal() == .alertSecondButtonReturn else { return .terminateCancel }
        }
        terminating = true
        dependencies.shutdownProcess { [weak self] in self?.finishTermination(application) }
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.shutdownDeadline) { [weak self] in
            guard let self, !terminationReplied else { return }
            Logger.app.error("Runtime shutdown exceeded its deadline; quitting anyway.")
            finishTermination(application)
        }
        return .terminateLater
    }

    private func finishTermination(_ application: NSApplication) {
        guard !terminationReplied else { return }
        terminationReplied = true
        application.reply(toApplicationShouldTerminate: true)
    }

    func applicationWillTerminate(_: Notification) {
        do {
            try (storageBootstrap.defaults as? EphemeralUserDefaults)?.cleanUp()
        } catch {
            Logger.app.error("Isolated preferences cleanup failed: \(error.localizedDescription)")
        }
    }

    func applicationShouldHandleReopen(_: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            MainWindowLocator.mainWindow()?.makeKeyAndOrderFront(nil)
        }
        return true
    }

    func applicationShouldTerminateAfterLastWindowClosed(_: NSApplication) -> Bool {
        false
    }

    func application(_: NSApplication, open urls: [URL]) {
        for url in urls {
            guard let destination = DeepLinkRouter.destination(for: url) else {
                Logger.app.warning("Ignored unsupported deep link: \(url)")
                continue
            }
            dependencies.pendingDeepLink = destination
            MainWindowLocator.activate()
        }
    }

    nonisolated func userNotificationCenter(
        _: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        let identifier = response.notification.request.identifier
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier else { return }
        await MainActor.run {
            guard self.dependencies.openTerminalNotification(identifier: identifier) else { return }
            MainWindowLocator.activate()
        }
    }

    static func correctedWindowFrame(
        _ frame: NSRect,
        visibleFrames: [NSRect],
        mainVisibleFrame: NSRect
    ) -> NSRect {
        let center = NSPoint(x: frame.midX, y: frame.midY)
        let attachedVisibleFrame = visibleFrames.first(where: { $0.contains(center) })
        let targetVisibleFrame = attachedVisibleFrame ?? mainVisibleFrame
        let width = min(max(frame.width, minimumWindowSize.width), targetVisibleFrame.width)
        let height = min(max(frame.height, minimumWindowSize.height), targetVisibleFrame.height)

        if attachedVisibleFrame != nil, width == frame.width, height == frame.height {
            return frame
        }

        let targetCenter = attachedVisibleFrame == nil
            ? NSPoint(x: targetVisibleFrame.midX, y: targetVisibleFrame.midY)
            : center
        let x = min(
            max(targetCenter.x - width / 2, targetVisibleFrame.minX),
            targetVisibleFrame.maxX - width
        )
        let y = min(
            max(targetCenter.y - height / 2, targetVisibleFrame.minY),
            targetVisibleFrame.maxY - height
        )
        return NSRect(x: x, y: y, width: width, height: height)
    }
}
