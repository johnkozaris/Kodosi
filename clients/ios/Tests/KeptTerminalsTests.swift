import Foundation
@testable import Kodosi
import Testing
import UserNotifications

struct KeptTerminalsTests {
    private func shown(_ terminal: String?, _ state: String = "blocked") -> UNNotificationRequest {
        let content = UNMutableNotificationContent()
        if let terminal {
            content.userInfo = ["kodosi": ["terminal": terminal, "agent": "claude", "state": state, "pushed": [terminal]]]
        }
        return UNNotificationRequest(identifier: terminal ?? "elsewhere", content: content, trigger: nil)
    }

    @Test func `a push takes back the notifications of the terminals that it does not name`() throws {
        let delivered = [shown("t-1"), shown("t-2", "done"), shown("t-3"), shown(nil)]
        let alert = try #require(KeptTerminals(pushed: [
            "aps": ["alert": ["loc-key": "A terminal finished"], "mutable-content": 1],
            "kodosi": ["terminal": "t-2", "agent": "aider", "state": "done", "kind": NSNull(), "pushed": ["t-2", "t-3"]],
        ]))
        #expect(alert.withdrawn(from: delivered) == ["t-1"])
        let background = try #require(KeptTerminals(pushed: ["aps": ["content-available": 1], "kodosi": ["pushed": [String]()]]))
        #expect(background.withdrawn(from: delivered) == ["t-1", "t-2", "t-3"])
        #expect(KeptTerminals(pushed: ["aps": ["alert": "Hello"]]) == nil)
    }

    @Test func `after a push the badge counts the waits whose notifications stay`() throws {
        let delivered = [shown("t-1"), shown("t-2", "done"), shown("t-3"), shown("t-3")]
        let pushed = try #require(KeptTerminals(pushed: ["aps": ["content-available": 1], "kodosi": ["pushed": ["t-2", "t-3"]]]))
        #expect(pushed.waiting(in: delivered) == 1)
    }
}
