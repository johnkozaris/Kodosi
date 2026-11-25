import Foundation

public enum DeviceCode {
    public static let length = 12

    public static func symbols(in text: String) -> String {
        String(text.uppercased().filter { $0.isASCII && ($0.isLetter || $0.isNumber) }.prefix(length))
    }

    public static func format(_ text: String) -> String {
        let symbols = Array(symbols(in: text))
        return stride(from: 0, to: symbols.count, by: 4)
            .map { String(symbols[$0 ..< min($0 + 4, symbols.count)]) }
            .joined(separator: "-")
    }

    public static func isComplete(_ text: String) -> Bool {
        symbols(in: text).count == length
    }

    /// Checks only the `kodosi://device/<way>/` prefix of links and QR scans; the runtime
    /// validates the payload.
    public static func scanned(in text: String, for way: ScannedWay) -> String? {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.hasPrefix("kodosi://device/\(way.rawValue)/") ? text : nil
    }

    public enum ScannedWay: String, Sendable {
        case join
        case approve
    }
}
