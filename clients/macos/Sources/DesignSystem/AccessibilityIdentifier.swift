import Foundation

enum AccessibilityIdentifier {
    static func token(_ value: String) -> String {
        Data(value.utf8)
            .base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    static func session(_ sessionId: String) -> String {
        "session.\(token(sessionId))"
    }

    static func sidebarSession(_ sessionId: String) -> String {
        "sidebar.\(session(sessionId))"
    }

    static func stageSession(_ sessionId: String) -> String {
        "stage.\(session(sessionId))"
    }
}
