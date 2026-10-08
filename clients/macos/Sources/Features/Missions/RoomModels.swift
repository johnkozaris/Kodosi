import Foundation

struct RoomSnapshot: Decodable {
    let roomId: String
    var messages: [RoomMessage]
    var tasks: [RoomTask]
    let repositories: [RoomRepository]
    let sequence: UInt64
    var hasOlder: Bool
    let moreTasks: Bool
}

struct RoomMessage: Decodable, Identifiable {
    let id: String
    let sequence: UInt64
    let authorId: String
    let authorName: String
    let agent: String?
    let terminalId: String?
    let text: String
    let createdAt: String
}

struct RoomTask: Decodable, Identifiable {
    let id: String
    let version: UInt64
    let title: String
    let description: String
    let closed: Bool
    let assignedTo: String?
    let assignedName: String?
    let terminalId: String?
    let repositoryIds: [String]
    let note: String?
    let issue: RoomIssue?
}

struct RoomRepository: Decodable, Identifiable {
    let id: String
    let name: String
    let url: String
    let host: String
    let owner: String
    let repository: String
    let provider: String
}

struct RoomIssue: Decodable, Identifiable {
    var id: String {
        "\(repositoryId):\(number)"
    }

    let repositoryId: String
    let number: UInt64
    let title: String
    let body: String
    let url: String
    let closed: Bool
    let assignees: [String]
}
