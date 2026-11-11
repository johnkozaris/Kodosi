import Foundation

/// The push names no terminal and no computer, so the words come from this device.
nonisolated struct TerminalAlert: Equatable {
    let terminal: String
    let agent: String?
    let state: String
    let kind: String?

    init?(_ userInfo: [AnyHashable: Any]) {
        guard let fields = userInfo["kodosi"] as? [String: Any], let terminal = fields["terminal"] as? String,
              let state = fields["state"] as? String
        else { return nil }
        self.terminal = terminal
        agent = fields["agent"] as? String
        self.state = state
        kind = fields["kind"] as? String
    }

    private var agentName: String? {
        switch agent {
        case "claude": "Claude Code"
        case "codex": "Codex"
        case "copilot": "Copilot CLI"
        case "cursor": "Cursor"
        case "gemini": "Gemini CLI"
        case "opencode": "OpenCode"
        default: nil
        }
    }

    func body(in bundle: Bundle) -> String {
        guard let name = agentName else {
            return switch (state, kind) {
            case ("blocked", "permission"): String(localized: "A terminal asks for permission", bundle: bundle)
            case ("blocked", "question"): String(localized: "A terminal asks you a question", bundle: bundle)
            case ("blocked", "auth"): String(localized: "A terminal needs you to sign in", bundle: bundle)
            case ("blocked", _): String(localized: "A terminal waits for you", bundle: bundle)
            case ("error", _): String(localized: "A terminal failed", bundle: bundle)
            default: String(localized: "A terminal finished", bundle: bundle)
            }
        }
        return switch (state, kind) {
        case ("blocked", "permission"): String(localized: "\(name) asks for permission", bundle: bundle)
        case ("blocked", "question"): String(localized: "\(name) asks you a question", bundle: bundle)
        case ("blocked", "auth"): String(localized: "\(name) needs you to sign in", bundle: bundle)
        case ("blocked", _): String(localized: "\(name) waits for you", bundle: bundle)
        case ("error", _): String(localized: "\(name) failed", bundle: bundle)
        default: String(localized: "\(name) finished", bundle: bundle)
        }
    }
}
