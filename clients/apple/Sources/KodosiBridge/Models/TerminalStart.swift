import Foundation

public struct TerminalStart: Equatable, Sendable {
    public var name: String?
    public var directory: String?
    public var computer: String?
    public var projectId: String?
    public var command: String?
    public var agent: AgentStart?
    public var like: String?
    public var prompt: String?
    public var branch: String?
    public var worktrees: String?
    public var size: JSONValue?

    public init(
        name: String? = nil, directory: String? = nil, computer: String? = nil, projectId: String? = nil, command: String? = nil,
        agent: AgentStart? = nil, like: String? = nil, prompt: String? = nil, branch: String? = nil, worktrees: String? = nil,
        size: JSONValue? = nil,
    ) {
        self.name = name
        self.directory = directory
        self.computer = computer
        self.projectId = projectId
        self.command = command
        self.agent = agent
        self.like = like
        self.prompt = prompt
        self.branch = branch
        self.worktrees = worktrees
        self.size = size
    }

    public var fields: [String: JSONValue] {
        var fields: [String: JSONValue] = [:]
        let words = [
            ("name", name), ("projectId", projectId), ("workingDir", directory), ("computer", computer), ("command", command), ("like", like),
            ("prompt", prompt), ("branch", branch), ("worktrees", worktrees),
        ]
        for case let (key, value?) in words {
            fields[key] = .string(value)
        }
        fields["agent"] = agent?.json
        fields["size"] = size
        return fields
    }
}
