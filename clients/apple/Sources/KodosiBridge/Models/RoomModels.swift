import Foundation

public struct RoomSnapshot: Decodable {
    public let roomId: String
    public var messages: [RoomMessage]
    public var tasks: [RoomTask]
    public let repositories: [RoomRepository]
    public var hasOlder: Bool
    public var historyTrimmed: Bool
}

public extension RoomSnapshot {
    mutating func merge(_ previous: RoomSnapshot) {
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

    func newMessages(after previous: RoomSnapshot, userId: String?) -> Int {
        let latest = previous.messages.map(\.sequence).max() ?? 0
        return messages.count { $0.sequence > latest && !$0.isWritten(by: userId) }
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

    public func isWritten(by userId: String?) -> Bool {
        agent == nil && authorId == userId
    }
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
}

public struct RoomIssue: Decodable, Identifiable {
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
