import Foundation

public enum RoomTaskChange: String, Sendable {
    case claim
    case release
    case close
    case reopen
}

public enum RoomAction {
    public static func post(_ text: String) -> [String: JSONValue] {
        ["type": .string("post"), "text": .string(text)]
    }

    public static func read(before: UInt64? = nil) -> [String: JSONValue] {
        var action: [String: JSONValue] = ["type": .string("read")]
        if let before {
            action["before"] = .uint(before)
        }
        return action
    }

    public static func createTask(title: String, description: String, repositoryIds: some Sequence<String>) -> [String: JSONValue] {
        [
            "type": .string("createTask"),
            "title": .string(clip(title.trimmingCharacters(in: .whitespacesAndNewlines), bytes: 256)),
            "description": .string(clip(description.trimmingCharacters(in: .whitespacesAndNewlines), bytes: 16384)),
            "repositoryIds": .array(repositoryIds.sorted().map(JSONValue.string)),
        ]
    }

    public static func createTask(from message: RoomMessage) -> [String: JSONValue] {
        let line = message.text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? message.text
        let author = message.agent ?? message.authorName
        return createTask(
            title: String(line.prefix(120)),
            description: String(localized: "From \(author): \(message.text)", bundle: .module),
            repositoryIds: [],
        )
    }

    public static func updateTask(_ taskId: String, change: RoomTaskChange, note: String? = nil) -> [String: JSONValue] {
        var action: [String: JSONValue] = ["type": .string("updateTask"), "taskId": .string(taskId), "change": .string(change.rawValue)]
        if let note = note?.trimmingCharacters(in: .whitespacesAndNewlines), !note.isEmpty {
            action["note"] = .string(note)
        }
        return action
    }

    public static func addRepository(_ address: String) -> [String: JSONValue] {
        ["type": .string("addRepository"), "url": .string(address.trimmingCharacters(in: .whitespacesAndNewlines))]
    }

    public static func issues(_ repositoryId: String) -> [String: JSONValue] {
        ["type": .string("issues"), "repositoryId": .string(repositoryId)]
    }

    public static func importIssue(repositoryId: String, number: UInt64) -> [String: JSONValue] {
        ["type": .string("importIssue"), "repositoryId": .string(repositoryId), "number": .uint(number)]
    }

    private static func clip(_ text: String, bytes: Int) -> String {
        guard text.utf8.count > bytes else { return text }
        var result = ""
        var used = 0
        for character in text {
            let size = String(character).utf8.count
            guard used + size <= bytes else { break }
            result.append(character)
            used += size
        }
        return result
    }
}
