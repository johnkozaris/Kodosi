import Foundation

enum TerminalTitle {
    static func sanitize(_ title: String?) -> String? {
        guard let title else { return nil }
        let sanitized = title.unicodeScalars
            .lazy
            .filter {
                !CharacterSet.controlCharacters.contains($0)
                    && !isBidiControl($0.value)
            }
            .prefix(512)
            .reduce(into: "") { result, scalar in
                result.unicodeScalars.append(scalar)
            }
        return sanitized.isEmpty ? nil : sanitized
    }

    private static func isBidiControl(_ value: UInt32) -> Bool {
        switch value {
        case 0x061C, 0x200E, 0x200F, 0x202A ... 0x202E, 0x2066 ... 0x2069:
            true
        default:
            false
        }
    }
}
