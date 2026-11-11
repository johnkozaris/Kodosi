import KodosiBridge
import Observation
import os
import UIKit
import UserNotifications

@Observable
final class PushNotifications: NSObject, UNUserNotificationCenterDelegate {
    enum Permission { case unknown, allowed, denied }

    private static let enabledKey = "notifications.enabled"
    private static let logger = Logger(subsystem: "com.kodosi.mobile", category: "push")

    private(set) var permission = Permission.unknown
    var enabled = UserDefaults.standard.object(forKey: enabledKey) as? Bool ?? true {
        didSet {
            UserDefaults.standard.set(enabled, forKey: Self.enabledKey)
            if enabled {
                register()
            } else if ready {
                commands.perform("push.unregister")
            }
        }
    }

    var opened: String?

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private var ready = false
    @ObservationIgnored private var names: TerminalNames?

    init(commands: CommandSink) {
        self.commands = commands
        super.init()
        UNUserNotificationCenter.current().delegate = self
    }

    func accountChanged(ready: Bool) {
        self.ready = ready
        if ready {
            register()
        } else {
            names = nil
            TerminalNames.forget()
        }
    }

    func register() {
        guard ready, enabled else { return }
        Task {
            let center = UNUserNotificationCenter.current()
            var status = await center.notificationSettings().authorizationStatus
            if status == .notDetermined {
                _ = try? await center.requestAuthorization(options: [.alert, .sound, .badge])
                status = await center.notificationSettings().authorizationStatus
            }
            switch status {
            case .authorized, .provisional, .ephemeral:
                permission = .allowed
                UIApplication.shared.registerForRemoteNotifications()
            case .denied:
                permission = .denied
            default:
                permission = .unknown
            }
        }
    }

    func received(token: Data) {
        guard ready, enabled else { return }
        commands.perform("push.register", [
            "platform": .string("apns"),
            "token": .string(token.map { String(format: "%02x", $0) }.joined()),
            "environment": .string(Self.environment),
        ])
    }

    func failed(_ error: any Error) {
        Self.logger.error("This device got no push token: \(error.localizedDescription, privacy: .public)")
    }

    func remember(_ sessions: [RuntimeSession]) {
        guard ready else { return }
        var known = TerminalNames()
        for session in sessions where session.isOwner {
            known.terminals[session.id] = TerminalNames.Entry(name: session.name, computer: session.hostName)
        }
        guard known != names else { return }
        names = known
        known.write()
    }

    func showWaiting(_ count: Int) {
        UNUserNotificationCenter.current().setBadgeCount(count)
    }

    func seen(_ terminal: String) {
        Task {
            let center = UNUserNotificationCenter.current()
            let shown = await center.deliveredNotifications()
            center.removeDeliveredNotifications(withIdentifiers: shown.filter {
                $0.request.content.threadIdentifier == terminal
            }.map(\.request.identifier))
        }
    }

    private func open(_ terminal: String) {
        opened = terminal
    }

    nonisolated func userNotificationCenter(
        _: UNUserNotificationCenter, willPresent _: UNNotification,
    ) async -> UNNotificationPresentationOptions {
        []
    }

    nonisolated func userNotificationCenter(_: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        guard let terminal = TerminalAlert(response.notification.request.content.userInfo)?.terminal else { return }
        await open(terminal)
    }

    /// The APNs environment of this build's token: the one that its provisioning profile names,
    /// and production for the App Store and TestFlight, which carry no profile.
    private static var environment: String {
        #if targetEnvironment(simulator)
            return "development"
        #else
            guard let url = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
                  let text = (try? Data(contentsOf: url)).flatMap({ String(data: $0, encoding: .isoLatin1) }),
                  let start = text.range(of: "<plist"), let end = text.range(of: "</plist>"),
                  let profile = try? PropertyListSerialization.propertyList(
                      from: Data(text[start.lowerBound ..< end.upperBound].utf8), format: nil,
                  ) as? [String: Any],
                  let value = (profile["Entitlements"] as? [String: Any])?["aps-environment"] as? String
            else { return "production" }
            return value
        #endif
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate {
    weak static var push: PushNotifications?

    func application(_: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        Self.push?.received(token: deviceToken)
    }

    func application(_: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: any Error) {
        Self.push?.failed(error)
    }
}
