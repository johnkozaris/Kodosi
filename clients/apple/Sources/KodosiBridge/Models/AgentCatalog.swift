import Foundation

public struct AgentCatalog: Decodable, Equatable, Sendable {
    public var agents: [AgentInfo]
    public var kodosi: String?
}

public struct AgentInfo: Decodable, Equatable, Identifiable, Sendable {
    public var agent: String
    public var program: String
    public var installed: Bool
    public var accounts: [AgentAccount]
    public var install: String
    public var addsAccounts: Bool
    public var reportsUsage: Bool
    public var permissions: [AgentPermissions]
    public var outputs: [AgentOutput]

    public init(
        agent: String, program: String, installed: Bool, accounts: [AgentAccount], install: String, addsAccounts: Bool,
        reportsUsage: Bool, permissions: [AgentPermissions] = [.ask], outputs: [AgentOutput] = [],
    ) {
        self.agent = agent
        self.program = program
        self.installed = installed
        self.accounts = accounts
        self.install = install
        self.addsAccounts = addsAccounts
        self.reportsUsage = reportsUsage
        self.permissions = permissions
        self.outputs = outputs
    }

    public var id: String {
        agent
    }
}

public struct AgentAccount: Decodable, Equatable, Identifiable, Sendable {
    public var name: String
    public var command: String
    public var identity: String?
    public var folder: String?

    public init(name: String, command: String, identity: String? = nil, folder: String? = nil) {
        self.name = name
        self.command = command
        self.identity = identity
        self.folder = folder
    }

    public var id: String {
        name
    }

    public var signIn: String {
        folder ?? command
    }

    public func title(program: String) -> String {
        guard name != program else { return String(localized: "Default", bundle: .module) }
        var base = Substring(name)
        if base.count > program.count + 1, base.hasPrefix(program), ["-", "_"].contains(base.dropFirst(program.count).first) {
            base = base.dropFirst(program.count + 1)
        }
        return base.prefix(1).uppercased() + base.dropFirst()
    }
}

public struct AgentModels: Decodable, Equatable, Sendable {
    public var models: [AgentModel]
    public var efforts: [AgentOption]

    public init(models: [AgentModel], efforts: [AgentOption]) {
        self.models = models
        self.efforts = efforts
    }

    public func efforts(for model: String?) -> [AgentOption] {
        guard let allowed = models.first(where: { $0.id == model })?.efforts, !allowed.isEmpty else { return efforts }
        return efforts.filter { allowed.contains($0.id) }
    }
}

public struct AgentModel: Decodable, Equatable, Identifiable, Sendable {
    public var id: String
    public var label: String
    public var efforts: [String]

    public init(id: String, label: String, efforts: [String]) {
        self.id = id
        self.label = label
        self.efforts = efforts
    }
}

public struct AgentOption: Decodable, Equatable, Identifiable, Sendable {
    public var id: String

    public init(id: String) {
        self.id = id
    }

    public var label: String {
        switch id {
        case "none": String(localized: "None", bundle: .module)
        case "minimal": String(localized: "Minimal", bundle: .module)
        case "low": String(localized: "Low", bundle: .module)
        case "medium": String(localized: "Medium", bundle: .module)
        case "high": String(localized: "High", bundle: .module)
        case "xhigh": String(localized: "Extra high", bundle: .module)
        case "max": String(localized: "Max", bundle: .module)
        default: id.prefix(1).uppercased() + id.dropFirst()
        }
    }
}

public struct AgentUsageReply: Decodable, Sendable {
    public var accounts: [AccountUsage]
}

public struct AccountUsage: Decodable, Equatable, Sendable {
    public var agent: String
    public var account: String
    public var limits: [UsageLimit]
    public var readAtMs: UInt64

    public init(agent: String, account: String, limits: [UsageLimit], readAtMs: UInt64) {
        self.agent = agent
        self.account = account
        self.limits = limits
        self.readAtMs = readAtMs
    }

    public var headline: UsageLimit? {
        limits.first(where: \.headline) ?? limits.first
    }

    public var stale: Bool {
        Date().timeIntervalSince1970 - Double(readAtMs) / 1000 > 60
    }

    public static func key(_ agent: String, _ account: String) -> String {
        "\(agent)/\(account)"
    }
}

public struct UsageLimit: Decodable, Equatable, Identifiable, Sendable {
    public var hours: Int?
    public var scope: String?
    public var used: Int
    public var level: UsageLevel
    public var headline: Bool

    public init(hours: Int?, scope: String?, used: Int, level: UsageLevel, headline: Bool) {
        self.hours = hours
        self.scope = scope
        self.used = used
        self.level = level
        self.headline = headline
    }

    public var id: String {
        scope.map { "scope:\($0)" } ?? hours.map { "hours:\($0)" } ?? "month"
    }

    public var window: String {
        if let scope {
            return scope
        }
        switch hours {
        case nil: return String(localized: "month", bundle: .module)
        case 24: return String(localized: "day", bundle: .module)
        case 168: return String(localized: "week", bundle: .module)
        case let hours? where hours % 24 == 0: return String(localized: "\(hours / 24)d", bundle: .module)
        case let hours?: return String(localized: "\(hours)h", bundle: .module)
        }
    }

    public var spoken: String {
        let name = switch (scope, hours) {
        case (nil, nil): String(localized: "This month", bundle: .module)
        case (nil, 168): String(localized: "This week", bundle: .module)
        case let (nil, hours?): String(localized: "Last \(hours) hours", bundle: .module)
        case let (scope?, nil): String(localized: "\(scope), this month", bundle: .module)
        case let (scope?, 168): String(localized: "\(scope), this week", bundle: .module)
        case let (scope?, hours?): String(localized: "\(scope), last \(hours) hours", bundle: .module)
        }
        return String(localized: "\(name): \(used)% used", bundle: .module)
    }

    public static func nearest(of limits: [Self]) -> Self? {
        limits.max { ($0.level, $0.used) < ($1.level, $1.used) }
    }
}

public enum UsageLevel: String, Decodable, Comparable, Sendable {
    case normal, near, reached

    private var rank: Int {
        switch self {
        case .normal: 0
        case .near: 1
        case .reached: 2
        }
    }

    public static func < (lhs: Self, rhs: Self) -> Bool {
        lhs.rank < rhs.rank
    }
}

/// The agent that `session.create` starts in the new terminal, with the choices of the launcher.
public struct AgentStart: Equatable, Sendable {
    public var agent: String
    public var account: String?
    public var model: String?
    public var effort: String?
    public var permissions: AgentPermissions?
    public var output: AgentOutput?

    public init(
        agent: String, account: String? = nil, model: String? = nil, effort: String? = nil,
        permissions: AgentPermissions? = nil, output: AgentOutput? = nil,
    ) {
        self.agent = agent
        self.account = account
        self.model = model
        self.effort = effort
        self.permissions = permissions
        self.output = output
    }

    public var json: JSONValue {
        var fields: [String: JSONValue] = ["agent": .string(agent)]
        fields["account"] = account.map(JSONValue.string)
        fields["model"] = model.map(JSONValue.string)
        fields["effort"] = effort.map(JSONValue.string)
        fields["permissions"] = permissions.map { .string($0.rawValue) }
        fields["output"] = output.map { .string($0.rawValue) }
        return .object(fields)
    }
}

/// What the launcher starts for an agent: its account, and the model and effort that the agent
/// still offers.
@MainActor
public struct LaunchPlan {
    public let info: AgentInfo?
    public let pick: AgentPick?
    public let models: AgentModels?
    private let usage: [String: AccountUsage]

    public init(launcher: LauncherState, agent: String? = nil) {
        let agent = agent ?? launcher.choice.agent
        info = launcher.agent(agent)
        pick = agent.map(launcher.choice.pick)
        models = agent.flatMap { launcher.models[$0] }
        usage = launcher.usage
    }

    public func usage(of account: AgentAccount?) -> UsageLimit? {
        guard let info, let account, account.identity != nil else { return nil }
        return usage[AccountUsage.key(info.agent, account.name)]?.headline
    }

    public var account: AgentAccount? {
        guard let info else { return nil }
        return info.accounts.first { $0.name == pick?.account } ?? info.accounts.first
    }

    public var modelId: String? {
        guard let saved = pick?.model else { return nil }
        guard let models else { return saved }
        return models.models.contains { $0.id == saved } ? saved : nil
    }

    public var model: AgentModel? {
        models?.models.first { $0.id == modelId }
    }

    public var efforts: [AgentOption] {
        models?.efforts(for: modelId) ?? []
    }

    public var effortId: String? {
        guard let saved = pick?.effort else { return nil }
        guard models != nil else { return saved }
        return efforts.contains { $0.id == saved } ? saved : nil
    }

    public var listsModels: Bool {
        models == nil || !(models?.models.isEmpty ?? true)
    }

    /// How the agent asks before it acts: the last choice that it still has, else asking.
    public var permissions: AgentPermissions {
        guard let info, let saved = pick?.permissions, info.permissions.contains(saved) else { return .ask }
        return saved
    }

    /// How much of its work the agent shows: the last choice that it still has, else its standard.
    public var output: AgentOutput {
        guard let info, let saved = pick?.output, info.outputs.contains(saved) else { return .standard }
        return saved
    }

    public var shown: AgentPick? {
        info.map {
            AgentPick(
                agent: $0.agent, account: account?.name, model: modelId, effort: effortId, permissions: permissions,
                output: output,
            )
        }
    }

    public var start: AgentStart? {
        guard let info, info.installed, let account else { return nil }
        return AgentStart(
            agent: info.agent, account: account.name, model: modelId ?? "", effort: effortId ?? "", permissions: permissions,
            output: info.outputs.isEmpty ? nil : output,
        )
    }
}
