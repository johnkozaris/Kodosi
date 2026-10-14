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
