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
    var reportsUsage: Bool

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

struct AgentUsageReply: Decodable {
    var accounts: [AccountUsage]
}

struct AccountUsage: Decodable, Equatable {
    var agent: String
    var account: String
    var limits: [UsageLimit]
    var readAtMs: UInt64

    var headline: UsageLimit? {
        limits.first(where: \.headline) ?? limits.first
    }

    var stale: Bool {
        Date().timeIntervalSince1970 - Double(readAtMs) / 1000 > 600
    }

    static func key(_ agent: String, _ account: String) -> String {
        "\(agent)/\(account)"
    }
}

struct UsageLimit: Decodable, Equatable, Identifiable {
    var hours: Int?
    var scope: String?
    var used: Int
    var level: UsageLevel
    var headline: Bool

    var id: String {
        scope.map { "scope:\($0)" } ?? hours.map { "hours:\($0)" } ?? "month"
    }

    var window: String {
        if let scope {
            return scope
        }
        switch hours {
        case nil: return String(localized: "month")
        case 24: return String(localized: "day")
        case 168: return String(localized: "week")
        case let hours? where hours % 24 == 0: return String(localized: "\(hours / 24)d")
        case let hours?: return String(localized: "\(hours)h")
        }
    }

    var spoken: String {
        let window = switch hours {
        case nil: String(localized: "This month")
        case 168: String(localized: "This week")
        case let hours?: String(localized: "Last \(hours) hours")
        }
        let name = scope.map { "\($0), \(window.lowercased())" } ?? window
        return String(localized: "\(name): \(used)% used")
    }

    /// The limit nearest to stopping its account: the worst level, then the most use; the first one wins a tie.
    static func nearest(of limits: [Self]) -> Self? {
        limits.max { ($0.level, $0.used) < ($1.level, $1.used) }
    }
}

enum UsageLevel: String, Decodable, Comparable {
    case normal, near, reached

    private var rank: Int {
        switch self {
        case .normal: 0
        case .near: 1
        case .reached: 2
        }
    }

    static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rank < rhs.rank
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
    private let usage: [String: AccountUsage]

    init(launcher: LauncherState, agent: String? = nil) {
        let agent = agent ?? launcher.choice.agent
        info = launcher.agent(agent)
        pick = agent.map(launcher.choice.pick)
        models = agent.flatMap { launcher.models[$0] }
        usage = launcher.usage
    }

    func usage(of account: AgentAccount?) -> UsageLimit? {
        guard let info, let account, account.identity != nil else { return nil }
        return usage[AccountUsage.key(info.agent, account.name)]?.headline
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
