import Foundation

public enum AccessibilityIdentifier {
    public static func token(_ value: String) -> String {
        Data(value.utf8)
            .base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    public static func session(_ sessionId: String) -> String {
        "session.\(token(sessionId))"
    }

    public static func sidebarSession(_ sessionId: String) -> String {
        "sidebar.\(session(sessionId))"
    }

    public static func stageSession(_ sessionId: String) -> String {
        "stage.\(session(sessionId))"
    }
}
