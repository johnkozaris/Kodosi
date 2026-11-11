import Foundation

public struct AgentSettings: Decodable, Equatable, Sendable {
    public var model: String?
    public var effort: String?
    public var permissions: AgentPermissions?
    public var output: AgentOutput?

    public init(model: String? = nil, effort: String? = nil, permissions: AgentPermissions? = nil, output: AgentOutput? = nil) {
        self.model = model
        self.effort = effort
        self.permissions = permissions
        self.output = output
    }

    public var json: JSONValue {
        var fields: [String: JSONValue] = [:]
        fields["model"] = model.map(JSONValue.string)
        fields["effort"] = effort.map(JSONValue.string)
        fields["permissions"] = permissions.map { .string($0.rawValue) }
        fields["output"] = output.map { .string($0.rawValue) }
        return .object(fields)
    }
}

public struct AgentLive: Decodable, Equatable, Sendable {
    public let model: Bool
    public let effort: Bool
    public let permissions: [AgentPermissions]
    /// The agent also keeps a model or effort that it changes to for its next sessions.
    public let keeps: Bool

    public init(model: Bool = false, effort: Bool = false, permissions: [AgentPermissions] = [], keeps: Bool = false) {
        self.model = model
        self.effort = effort
        self.permissions = permissions
        self.keeps = keeps
    }
}
