import Foundation

public struct AgentChoice: Decodable, Equatable, Sendable {
    public var agent: String?
    public var picks: [AgentPick]

    public init(agent: String? = nil, picks: [AgentPick] = []) {
        self.agent = agent
        self.picks = picks
    }

    public func pick(_ agent: String) -> AgentPick {
        picks.first { $0.agent == agent } ?? AgentPick(agent: agent)
    }

    public mutating func apply(_ change: AgentChoiceChange) {
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

public struct AgentPick: Decodable, Equatable, Sendable {
    public var agent: String
    public var account: String?
    public var model: String?
    public var effort: String?

    public init(agent: String, account: String? = nil, model: String? = nil, effort: String? = nil) {
        self.agent = agent
        self.account = account
        self.model = model
        self.effort = effort
    }
}

public enum AgentChoiceChange: Equatable, Sendable {
    case tab(String?)
    case pick(AgentPick)

    public var json: JSONValue {
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
