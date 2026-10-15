import Foundation

public enum RoomTaskAction {
    public static func create(title: String, description: String, repositoryIds: some Sequence<String>) -> [String: JSONValue] {
        [
            "type": .string("createTask"),
            "title": .string(clip(title.trimmingCharacters(in: .whitespacesAndNewlines), bytes: 256)),
            "description": .string(clip(description.trimmingCharacters(in: .whitespacesAndNewlines), bytes: 16384)),
            "repositoryIds": .array(repositoryIds.sorted().map(JSONValue.string)),
        ]
    }

    public static func create(from message: RoomMessage) -> [String: JSONValue] {
        let line = message.text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? message.text
        let author = message.agent ?? message.authorName
        return create(
            title: String(line.prefix(120)),
            description: String(localized: "From \(author): \(message.text)", bundle: .module),
            repositoryIds: []
        )
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
