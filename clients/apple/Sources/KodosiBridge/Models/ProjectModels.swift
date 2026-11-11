import Foundation

public struct ProjectSnapshot: Decodable {
    public let projectId: String
    public var messages: [ProjectMessage]
    public var tasks: [ProjectTask]
    public let repositories: [ProjectRepository]
    public var hasOlder: Bool
    public var historyTrimmed: Bool
}

public extension ProjectSnapshot {
    mutating func merge(_ previous: ProjectSnapshot) {
        hasOlder = previous.hasOlder && hasOlder
        historyTrimmed = previous.historyTrimmed || historyTrimmed
        var messages = Dictionary(previous.messages.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        for message in self.messages {
            messages[message.id] = message
        }
        self.messages = messages.values.sorted { $0.sequence < $1.sequence }
        var tasks = Dictionary(previous.tasks.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        for task in self.tasks where tasks[task.id].map({ $0.version <= task.version }) ?? true {
            tasks[task.id] = task
        }
        self.tasks = tasks.values.sorted { $0.id < $1.id }
    }

    func newMessages(after previous: ProjectSnapshot, userId: String?) -> Int {
        let latest = previous.messages.map(\.sequence).max() ?? 0
        return messages.count { $0.sequence > latest && !$0.isWritten(by: userId) }
    }
}

public struct ProjectMessage: Decodable, Identifiable {
    public let id: String
    public let sequence: UInt64
    public let authorId: String
    public let authorName: String
    public let agent: String?
    public let terminalId: String?
    public let text: String
    public let createdAt: String

    public func isWritten(by userId: String?) -> Bool {
        agent == nil && authorId == userId
    }
}

public struct ProjectTask: Decodable, Identifiable {
    public let id: String
    public let version: UInt64
    public let title: String
    public let description: String
    public let closed: Bool
    public let assignedTo: String?
    public let assignedName: String?
    public let terminalId: String?
    public let repositoryIds: [String]
    public let note: String?
    public let issue: ProjectIssue?
}

public struct ProjectTaskLanes {
    public let tasks: [ProjectTask]

    public init(_ tasks: [ProjectTask], repository: String?) {
        self.tasks = repository.map { id in tasks.filter { $0.repositoryIds.contains(id) } } ?? tasks
    }

    public var upForGrabs: [ProjectTask] {
        tasks.filter { !$0.closed && $0.assignedTo == nil }
    }

    public var inProgress: [ProjectTask] {
        tasks.filter { !$0.closed && $0.assignedTo != nil }
    }

    public var done: [ProjectTask] {
        tasks.filter(\.closed)
    }
}

public struct ProjectRepository: Decodable, Identifiable {
    public let id: String
    public let name: String
    public let url: String
    public let host: String
    public let owner: String
    public let repository: String
}

public struct ProjectIssue: Decodable, Identifiable {
    public var id: String {
        "\(repositoryId):\(number)"
    }

    public let repositoryId: String
    public let number: UInt64
    public let title: String
    public let url: String
    public let closed: Bool
    public let assignees: [String]

    public var address: URL? {
        guard let url = URL(string: url), ["https", "http"].contains(url.scheme?.lowercased() ?? "") else { return nil }
        return url
    }
}
