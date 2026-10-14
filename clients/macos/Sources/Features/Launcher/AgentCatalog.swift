import Foundation
import KodosiDesign

struct AgentCatalog: Decodable, Equatable {
    var agents: [AgentInfo]
}

struct AgentInfo: Decodable, Equatable, Identifiable {
    var agent: String
    var program: String
    var installed: Bool
    var accounts: [AgentAccount]
    var install: String
    var addsAccounts: Bool

    var id: String {
        agent
    }

    var kind: AgentKind {
        AgentKind(rawValue: agent) ?? .shell
    }
}

struct AgentAccount: Decodable, Equatable, Identifiable {
    var name: String
    var command: String
    var identity: String?

    var id: String {
        command
    }
}

struct AgentModels: Decodable, Equatable {
    var agent: String
    var models: [AgentModel]
    var efforts: [AgentOption]
}

struct AgentModel: Decodable, Equatable, Identifiable {
    var id: String
    var label: String
    var argument: String
    var efforts: [String]
}

struct AgentOption: Decodable, Equatable, Identifiable {
    var id: String
    var argument: String

    var label: String {
        switch id {
        case "xhigh": String(localized: "Extra high")
        default: id.prefix(1).uppercased() + id.dropFirst()
        }
    }
}

/// What the launcher starts: the agent (none for a plain terminal) and, for
/// each agent, the words of the account, model and effort chosen last.
struct LaunchChoice: Codable, Equatable {
    var agent: String?
    var accounts: [String: String] = [:]
    var models: [String: String] = [:]
    var efforts: [String: String] = [:]
}

enum CommandText {
    private static let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path

    /// `command` as a person reads it: no `env`, and `~` for the home folder.
    static func readable(_ command: String) -> String {
        var text = command
        if text.hasPrefix("env ") {
            text.removeFirst(4)
        }
        return text.replacingOccurrences(of: home + "/", with: "~/")
    }

    static func tilde(_ path: String) -> String {
        path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }
}
