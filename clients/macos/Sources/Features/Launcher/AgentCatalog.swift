import Foundation
import KodosiBridge
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
        name
    }

    var signIn: String {
        folder ?? command
    }

    func title(program: String) -> String {
        guard name != program else { return String(localized: "Default") }
        var base = Substring(name)
        if base.count > program.count + 1, base.hasPrefix(program), ["-", "_"].contains(base.dropFirst(program.count).first) {
            base = base.dropFirst(program.count + 1)
        }
        return base.prefix(1).uppercased() + base.dropFirst()
    }
}

struct AgentModels: Decodable, Equatable {
    var models: [AgentModel]
    var efforts: [AgentOption]

    func efforts(for model: String?) -> [AgentOption] {
        guard let allowed = models.first(where: { $0.id == model })?.efforts, !allowed.isEmpty else { return efforts }
        return efforts.filter { allowed.contains($0.id) }
    }
}

struct AgentModel: Decodable, Equatable, Identifiable {
    var id: String
    var label: String
    var efforts: [String]
}

struct AgentOption: Decodable, Equatable, Identifiable {
    var id: String

    var label: String {
        switch id {
        case "xhigh": String(localized: "Extra high")
        default: id.prefix(1).uppercased() + id.dropFirst()
        }
    }
}

struct AgentChoice: Decodable, Equatable {
    var agent: String?
    var picks: [AgentPick] = []

    func pick(_ agent: String) -> AgentPick {
        picks.first { $0.agent == agent } ?? AgentPick(agent: agent)
    }

    mutating func apply(_ change: AgentChoiceChange) {
        switch change {
        case let .tab(agent):
            self.agent = agent
        case let .pick(pick):
            if let index = picks.firstIndex(where: { $0.agent == pick.agent }) {
                picks[index] = pick
            } else {
                picks.append(pick)
            }
        }
    }
}

struct AgentPick: Decodable, Equatable {
    var agent: String
    var account: String?
    var model: String?
    var effort: String?
}

enum AgentChoiceChange: Equatable {
    case tab(String?)
    case pick(AgentPick)

    var json: JSONValue {
        switch self {
        case let .tab(agent):
            .object(["type": .string("tab"), "agent": agent.map(JSONValue.string) ?? .null])
        case let .pick(pick):
            .object([
                "type": .string("pick"), "agent": .string(pick.agent), "account": pick.account.map(JSONValue.string) ?? .null,
                "model": pick.model.map(JSONValue.string) ?? .null, "effort": pick.effort.map(JSONValue.string) ?? .null,
            ])
        }
    }
}

struct AgentStart: Equatable {
    var agent: String
    var account: String?
    var model: String?
    var effort: String?

    var json: JSONValue {
        var fields: [String: JSONValue] = ["agent": .string(agent)]
        fields["account"] = account.map(JSONValue.string)
        fields["model"] = model.map(JSONValue.string)
        fields["effort"] = effort.map(JSONValue.string)
        return .object(fields)
    }
}

@MainActor
struct LaunchPlan {
    let info: AgentInfo?
    let pick: AgentPick?
    let models: AgentModels?

    init(launcher: LauncherState, agent: String? = nil) {
        let agent = agent ?? launcher.choice.agent
        info = launcher.agent(agent)
        pick = agent.map(launcher.choice.pick)
        models = agent.flatMap { launcher.models[$0] }
    }

    var account: AgentAccount? {
        guard let info else { return nil }
        return info.accounts.first { $0.name == pick?.account } ?? info.accounts.first
    }

    var modelId: String? {
        guard let saved = pick?.model else { return nil }
        guard let models else { return saved }
        return models.models.contains { $0.id == saved } ? saved : nil
    }

    var model: AgentModel? {
        models?.models.first { $0.id == modelId }
    }

    var efforts: [AgentOption] {
        models?.efforts(for: modelId) ?? []
    }

    var effortId: String? {
        guard let saved = pick?.effort else { return nil }
        guard models != nil else { return saved }
        return efforts.contains { $0.id == saved } ? saved : nil
    }

    var listsModels: Bool {
        models == nil || !(models?.models.isEmpty ?? true)
    }

    var shown: AgentPick? {
        info.map { AgentPick(agent: $0.agent, account: account?.name, model: modelId, effort: effortId) }
    }

    var start: AgentStart? {
        guard let info, info.installed, let account else { return nil }
        return AgentStart(agent: info.agent, account: account.name, model: modelId ?? "", effort: effortId ?? "")
    }
}
