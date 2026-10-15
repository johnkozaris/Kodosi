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
    /// The folder that keeps the sign-in; commands with the same folder share one.
    var folder: String?
    /// The options that the command adds, such as `--yolo`.
    var options: String?

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

/// What the launcher would start now: the person's choice, held against what
/// this computer offers. A choice that the agent no longer offers falls back
/// to the agent's own default.
@MainActor
struct LaunchPlan {
    let choice: LaunchChoice
    let info: AgentInfo?
    let models: AgentModels?

    init(choice: LaunchChoice, launcher: LauncherState) {
        self.choice = choice
        info = launcher.agent(choice.agent)
        models = choice.agent.flatMap { launcher.models[$0] }
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

    /// Whether the agent lists models, or may still list them.
    var listsModels: Bool {
        models == nil || !(models?.models.isEmpty ?? true)
    }

    /// The command line that the new terminal types.
    var line: String? {
        guard let info else { return nil }
        guard info.installed else { return info.install }
        guard let account else { return nil }
        return ([account.command] + [modelArgument, effortArgument].compactMap(\.self)).joined(separator: " ")
    }
}
