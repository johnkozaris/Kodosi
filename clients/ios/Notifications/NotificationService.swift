import Foundation
@preconcurrency import UserNotifications

/// Push payloads from the server carry only opaque identifiers and state, so terminal names and
/// localized notification text are resolved locally on the device.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(
        _ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void,
    ) {
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent,
              let alert = TerminalAlert(request.content.userInfo)
        else {
            contentHandler(request.content)
            return
        }
        let names = TerminalNames.read()?.terminals[alert.terminal]
        if let names {
            content.title = names.name
            content.subtitle = names.computer ?? ""
        }
        content.body = alert.body(in: .app)
        content.threadIdentifier = names?.computerId ?? ""
        guard let pushed = KeptTerminals(pushed: request.content.userInfo) else {
            contentHandler(content)
            return
        }
        // The SDK omits @Sendable on contentHandler, even though it may be called on any thread.
        nonisolated(unsafe) let deliver = contentHandler
        let center = UNUserNotificationCenter.current()
        center.getDeliveredNotifications { delivered in
            center.removeDeliveredNotifications(withIdentifiers: pushed.withdrawn(from: delivered.map(\.request)))
            // Sending badge 0 in an APNs payload clears all delivered notifications, so the server
            // omits the badge when no terminal is waiting and we reset it locally instead.
            if content.badge == nil {
                center.setBadgeCount(0)
            }
            // Calling contentHandler lets iOS terminate the extension immediately, so issue a
            // follow-up query as a barrier to ensure the removal above completes first.
            center.getDeliveredNotifications { _ in deliver(content) }
        }
    }
}

private extension Bundle {
    /// Host app bundle (two levels above `.appex`), used to load the app's localized strings.
    static let app = Bundle(url: Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()) ?? .main
}
