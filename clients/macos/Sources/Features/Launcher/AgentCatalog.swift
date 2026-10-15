import Foundation
import KodosiDesign

struct AgentCatalog: Decodable, Equatable {
    var agents: [AgentInfo]
    var kodosi: String?
}

struct AgentInfo: Decodable, Equatable, Identifiable {
    var agent: String
    var program: String
    var installed: Bool
    var accounts: [AgentAccount]
    var install: String
    var addsAccounts: Bool
    var promptFlag: String?

    var id: String {
        agent
    }

    var kind: AgentKind {
        AgentKind(program: agent)
    }
}

struct AgentAccount: Decodable, Equatable, Identifiable {
    var name: String
    var command: String
    var identity: String?
    var folder: String?

    var id: String {
        command
    }

    var title: String {
        identity ?? folder ?? name
    }

    var detail: String {
        identity == nil ? String(localized: "Not signed in") : folder ?? ""
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

struct LaunchChoice: Codable, Equatable {
    var agent: String?
    var accounts: [String: String] = [:]
    var models: [String: String] = [:]
    var efforts: [String: String] = [:]
}

@MainActor
struct LaunchPlan {
    let choice: LaunchChoice
    let info: AgentInfo?
    let models: AgentModels?

    init(choice: LaunchChoice, launcher: LauncherState, agent: String? = nil) {
        self.choice = choice
        let agent = agent ?? choice.agent
        info = launcher.agent(agent)
        models = agent.flatMap { launcher.models[$0] }
    }

    var account: AgentAccount? {
        guard let info else { return nil }
        return info.accounts.first { $0.command == choice.accounts[info.agent] } ?? info.accounts.first
    }

    var modelArgument: String? {
        guard let info, let saved = choice.models[info.agent] else { return nil }
        guard let models else { return saved }
        return models.models.contains { $0.argument == saved } ? saved : nil
    }

    var model: AgentModel? {
        models?.models.first { $0.argument == modelArgument }
    }

    var efforts: [AgentOption] {
        let all = models?.efforts ?? []
        guard let model, !model.efforts.isEmpty else { return all }
        return all.filter { model.efforts.contains($0.id) }
    }

    var effortArgument: String? {
        guard let info, let saved = choice.efforts[info.agent] else { return nil }
        guard models != nil else { return saved }
        return efforts.contains { $0.argument == saved } ? saved : nil
    }

    var line: String? {
        guard let info else { return nil }
        guard info.installed else { return info.install }
        guard let account else { return nil }
        return ([account.command] + [modelArgument, effortArgument].compactMap(\.self)).joined(separator: " ")
    }
}
