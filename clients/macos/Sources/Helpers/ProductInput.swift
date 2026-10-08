import Foundation

enum ProductInput {
    static func validName(_ value: String) -> Bool {
        let name = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return !name.isEmpty && name.utf8.count <= 128 && !name.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
    }
}
