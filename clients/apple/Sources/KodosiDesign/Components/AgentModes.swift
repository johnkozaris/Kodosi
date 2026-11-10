import Foundation
import KodosiBridge

public extension AgentPermissions {
    var label: String {
        switch self {
        case .ask: String(localized: "Ask", bundle: .module)
        case .skip: String(localized: "Skip", bundle: .module)
        case .auto: String(localized: "Auto", bundle: .module)
        }
    }

    var symbol: String {
        switch self {
        case .ask: "hand.raised"
        case .skip: "bolt.shield"
        case .auto: "wand.and.stars"
        }
    }

    func detail(for kind: AgentKind) -> String {
        switch (self, kind) {
        case (.ask, _): String(localized: "Asks before it changes files or runs commands.", bundle: .module)
        case (.skip, _): String(localized: "Never asks, and runs every command as you.", bundle: .module)
        case (.auto, .claude), (.auto, .copilot):
            String(localized: "A safety check approves what is safe and asks you for the rest. The check uses tokens.", bundle: .module)
        case (.auto, .codex): String(localized: "Works in this folder on its own and asks you for the rest.", bundle: .module)
        case (.auto, .gemini): String(localized: "Edits files on its own and asks before it runs commands.", bundle: .module)
        case (.auto, _): String(localized: "Accepts its own actions, except what you denied.", bundle: .module)
        }
    }
}

public extension AgentOutput {
    var label: String {
        switch self {
        case .quiet: String(localized: "Quiet", bundle: .module)
        case .standard: String(localized: "Standard", bundle: .module)
        case .detailed: String(localized: "Detailed", bundle: .module)
        }
    }

    var detail: String {
        switch self {
        case .quiet: String(localized: "Shows less of its work.", bundle: .module)
        case .standard: String(localized: "Shows its work as the agent is set up.", bundle: .module)
        case .detailed: String(localized: "Shows each step of its work in full.", bundle: .module)
        }
    }
}
