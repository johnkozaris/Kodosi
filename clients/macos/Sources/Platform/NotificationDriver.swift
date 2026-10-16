import Foundation
@preconcurrency import UserNotifications

protocol DesktopNotificationDriving: Sendable {
    func getAuthorizationStatus(
        _ completion: @escaping @Sendable (UNAuthorizationStatus) -> Void
    )
    func requestAuthorization() async throws -> Bool
    func add(_ request: UNNotificationRequest, completion: @escaping @Sendable (Error?) -> Void)
    func getIdentifiers(_ completion: @escaping @Sendable ([String]) -> Void)
    func remove(identifiers: [String])
}

struct SystemNotificationDriver: DesktopNotificationDriving {
    let center = UNUserNotificationCenter.current()

    func getAuthorizationStatus(
        _ completion: @escaping @Sendable (UNAuthorizationStatus) -> Void
    ) {
        center.getNotificationSettings { completion($0.authorizationStatus) }
    }

    func requestAuthorization() async throws -> Bool {
        try await center.requestAuthorization(options: [.alert, .sound])
    }

    func add(_ request: UNNotificationRequest, completion: @escaping @Sendable (Error?) -> Void) {
        center.add(request, withCompletionHandler: completion)
    }

    func getIdentifiers(_ completion: @escaping @Sendable ([String]) -> Void) {
        center.getDeliveredNotifications { notifications in
            let delivered = notifications.map(\.request.identifier)
            center.getPendingNotificationRequests { requests in
                completion(delivered + requests.map(\.identifier))
            }
        }
    }

    func remove(identifiers: [String]) {
        center.removePendingNotificationRequests(withIdentifiers: identifiers)
        center.removeDeliveredNotifications(withIdentifiers: identifiers)
    }
}
