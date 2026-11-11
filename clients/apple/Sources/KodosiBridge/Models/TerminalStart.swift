import Foundation

public struct TerminalStart: Equatable, Sendable {
    public var name: String
    public var directory: String?
    public var computer: String?
    public var roomId: String?
    public var command: String?
    public var agent: AgentStart?
    public var prompt: String?
    public var branch: String?
    public var worktrees: String?
    public var size: JSONValue?

    public init(
        name: String, directory: String? = nil, computer: String? = nil, roomId: String? = nil, command: String? = nil,
        agent: AgentStart? = nil, prompt: String? = nil, branch: String? = nil, worktrees: String? = nil, size: JSONValue? = nil,
    ) {
        self.name = name
        self.directory = directory
        self.computer = computer
        self.roomId = roomId
        self.command = command
        self.agent = agent
        self.prompt = prompt
        self.branch = branch
        self.worktrees = worktrees
        self.size = size
    }

    public var fields: [String: JSONValue] {
        var fields: [String: JSONValue] = ["name": .string(name)]
        let words = [
            ("missionId", roomId), ("workingDir", directory), ("computer", computer), ("command", command), ("prompt", prompt),
            ("branch", branch), ("worktrees", worktrees),
        ]
        for case let (key, value?) in words {
            fields[key] = .string(value)
        }
        fields["agent"] = agent?.json
        fields["size"] = size
        return fields
    }
}
