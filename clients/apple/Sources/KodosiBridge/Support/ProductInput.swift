import Foundation

public enum ProductInput {
    public static let reportNoteBytes = 1000

    public static func validName(_ value: String) -> Bool {
        let name = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return !name.isEmpty && name.utf8.count <= 128 && !name.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
    }

    public static func clip(_ text: String, bytes: Int) -> String {
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
