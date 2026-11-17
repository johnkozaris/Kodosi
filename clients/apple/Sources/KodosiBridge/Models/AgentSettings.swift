import Foundation

public struct AgentSettings: Decodable, Equatable, Sendable {
    public var model: String?
    public var effort: String?
    public var permissions: AgentPermissions?
    public var output: AgentOutput?
    public var fast: Bool?

    public init(model: String? = nil, effort: String? = nil, permissions: AgentPermissions? = nil, output: AgentOutput? = nil, fast: Bool? = nil) {
        self.model = model
        self.effort = effort
        self.permissions = permissions
        self.output = output
        self.fast = fast
    }

    public var json: JSONValue {
        var fields: [String: JSONValue] = [:]
        fields["model"] = model.map(JSONValue.string)
        fields["effort"] = effort.map(JSONValue.string)
        fields["permissions"] = permissions.map { .string($0.rawValue) }
        fields["output"] = output.map { .string($0.rawValue) }
        fields["fast"] = fast.map(JSONValue.bool)
        return .object(fields)
    }
}

public struct AgentLive: Decodable, Equatable, Sendable {
    public let model: Bool
    public let effort: Bool
    public let fast: Bool
    public let permissions: [AgentPermissions]
    /// The agent also keeps a model, effort or fast mode that it changes to for its next sessions.
    public let keeps: Bool
}
