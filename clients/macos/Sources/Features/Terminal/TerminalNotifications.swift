import Foundation
import UserNotifications

@MainActor
final class TerminalNotifications {
    private let driver: any DesktopNotificationDriving
    private var lifetime = UUID().uuidString
    private var current: [String: RuntimeSession] = [:]
    private var pending: Set<String> = []
    private var issued: Set<String> = []
    private var alerts: Set<String> = []
    private var accountEpoch: UInt64 = 0
    private var activated = false

    init(driver: any DesktopNotificationDriving = SystemNotificationDriver()) {
        self.driver = driver
    }

    func reconcile(sessions: [RuntimeSession], accountEpoch: UInt64) {
        if self.accountEpoch != accountEpoch {
            clear()
            self.accountEpoch = accountEpoch
        }
        current = Dictionary(uniqueKeysWithValues: sessions.filter {
            ($0.kind == .local || $0.isOwner) && $0.status == .running
        }.map { ($0.id, $0) })
        let live = Set(current.values.map(identifier))
        let obsolete = issued.subtracting(live)
        issued.subtract(obsolete)
        alerts.subtract(obsolete)
        if !obsolete.isEmpty {
            driver.remove(identifiers: Array(obsolete))
        }
    }

    func activate() {
        guard !activated else { return }
        activated = true
        driver.getIdentifiers { [weak self] identifiers in
            Task { @MainActor in
                guard let self else { return }
                let obsolete = identifiers.filter {
                    $0.hasPrefix("terminal-") && !self.issued.contains($0)
                }
                if !obsolete.isEmpty {
                    self.driver.remove(identifiers: obsolete)
                }
            }
        }
    }

    func clear() {
        lifetime = UUID().uuidString
        current.removeAll()
        if !issued.isEmpty {
            driver.remove(identifiers: Array(issued))
            issued.removeAll()
        }
        alerts.removeAll()
    }

    func alert(_ text: String, session: RuntimeSession) {
        post(title: session.name, body: text, session: session, alert: true)
    }

    func withdrawAlert(_ sessionId: String) {
        guard let session = current[sessionId] else { return }
        let identifier = identifier(session)
        guard alerts.remove(identifier) != nil else { return }
        issued.remove(identifier)
        driver.remove(identifiers: [identifier])
    }

    func post(title: String?, body: String?, session: RuntimeSession, alert: Bool = false) {
        let identifier = identifier(session)
        guard current[session.id]?.incarnationId == session.incarnationId,
              issued.contains(identifier) || issued.count < 256,
              pending.count < 32, pending.insert(identifier).inserted else { return }
        issued.insert(identifier)
        if alert {
            alerts.insert(identifier)
        } else {
            alerts.remove(identifier)
        }
        let content = UNMutableNotificationContent()
        content.title = nonEmpty(title) ?? String(localized: "Terminal")
        content.body = nonEmpty(body) ?? session.name
        content.sound = .default
        let epoch = accountEpoch
        let request = UNNotificationRequest(identifier: identifier, content: content, trigger: nil)
        DesktopNotificationDelivery.addWhenAuthorized(
            request,
            driver: driver,
            remainsRelevant: { [weak self] in
                self?.accountEpoch == epoch && self?.issued.contains(identifier) == true && self?.current[session.id]?.incarnationId == session.incarnationId
            },
            failureContext: "Terminal notification",
            completion: { [weak self, driver] in
                Task { @MainActor in
                    self?.pending.remove(identifier)
                    if self?.accountEpoch != epoch || self?.issued.contains(identifier) != true ||
                        self?.current[session.id]?.incarnationId != session.incarnationId
                    {
                        driver.remove(identifiers: [identifier])
                    }
                }
            }
        )
    }

    func sessionToOpen(identifier: String) -> String? {
        guard issued.contains(identifier), let session = current.values.first(where: {
            self.identifier($0) == identifier
        }) else { return nil }
        alerts.remove(identifier)
        driver.remove(identifiers: [identifier])
        return session.id
    }

    private func identifier(_ session: RuntimeSession) -> String {
        "terminal-\(lifetime)-\(accountEpoch)-\(session.id)-\(session.incarnationId)"
    }

    private func nonEmpty(_ value: String?) -> String? {
        guard let value, !value.isEmpty else { return nil }
        return String(value.prefix(1024))
    }
}
