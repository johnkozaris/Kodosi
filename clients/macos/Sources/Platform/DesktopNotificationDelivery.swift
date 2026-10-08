import Foundation
import os
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

enum DesktopNotificationDelivery {
    static func addWhenAuthorized(
        _ request: UNNotificationRequest,
        remainsRelevant: @escaping @MainActor @Sendable () -> Bool = { true },
        failureContext: String,
        completion: @escaping @Sendable () -> Void = {}
    ) {
        addWhenAuthorized(
            request,
            driver: SystemNotificationDriver(),
            remainsRelevant: remainsRelevant,
            failureContext: failureContext,
            completion: completion
        )
    }

    static func addWhenAuthorized(
        _ request: UNNotificationRequest,
        driver: any DesktopNotificationDriving,
        remainsRelevant: @escaping @MainActor @Sendable () -> Bool = { true },
        failureContext: String,
        completion: @escaping @Sendable () -> Void = {}
    ) {
        driver.getAuthorizationStatus { status in
            Task { @MainActor in
                guard remainsRelevant() else { completion(); return }
                switch status {
                case .authorized, .provisional, .ephemeral:
                    add(request, using: driver, failureContext: failureContext, completion: completion)
                case .notDetermined:
                    do {
                        let granted = try await driver.requestAuthorization()
                        guard granted, remainsRelevant() else { completion(); return }
                        add(request, using: driver, failureContext: failureContext, completion: completion)
                    } catch {
                        Logger.app.warning("notification authorization failed: \(error.localizedDescription)")
                        completion()
                    }
                case .denied:
                    Logger.app.info("\(failureContext) skipped because notifications are disabled")
                    completion()
                @unknown default:
                    Logger.app.warning("\(failureContext) skipped for an unknown authorization status")
                    completion()
                }
            }
        }
    }

    private nonisolated static func add(
        _ request: UNNotificationRequest,
        using driver: any DesktopNotificationDriving,
        failureContext: String,
        completion: @escaping @Sendable () -> Void
    ) {
        driver.add(request) { error in
            defer { completion() }
            if let error {
                Logger.app.warning("\(failureContext) failed: \(error.localizedDescription)")
            }
        }
    }
}
