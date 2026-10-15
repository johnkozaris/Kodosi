import Foundation

public struct RoomSnapshot: Decodable {
    public let roomId: String
    public var messages: [RoomMessage]
    public var tasks: [RoomTask]
    public let repositories: [RoomRepository]
    public let sequence: UInt64
    public var hasOlder: Bool
}

public extension RoomSnapshot {
    mutating func merge(_ previous: RoomSnapshot) {
        hasOlder = previous.hasOlder && hasOlder
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
}

public struct RoomMessage: Decodable, Identifiable {
    public let id: String
    public let sequence: UInt64
    public let authorId: String
    public let authorName: String
    public let agent: String?
    public let terminalId: String?
    public let text: String
    public let createdAt: String
}

public struct RoomTask: Decodable, Identifiable {
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
    public let issue: RoomIssue?
}

public struct RoomRepository: Decodable, Identifiable {
    public let id: String
    public let name: String
    public let url: String
    public let host: String
    public let owner: String
    public let repository: String
    public let provider: String
}

public struct RoomIssue: Decodable, Identifiable {
    public var id: String {
        "\(repositoryId):\(number)"
    }

    public let repositoryId: String
    public let number: UInt64
    public let title: String
    public let body: String
    public let url: String
    public let closed: Bool
    public let assignees: [String]
}
