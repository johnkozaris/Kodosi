import Foundation
import UserNotifications

/// A push from the Kodosi server carries only identifiers, so the names and the words come from
/// this device.
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
        if let names = TerminalNames.read()?.terminals[alert.terminal] {
            content.title = names.name
            content.subtitle = names.computer ?? ""
        }
        content.body = alert.body(in: .app)
        content.threadIdentifier = alert.terminal
        contentHandler(content)
    }
}

private extension Bundle {
    /// The app that holds this extension, whose words the extension shows.
    static let app = Bundle(url: Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()) ?? .main
}
