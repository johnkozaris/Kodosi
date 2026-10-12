import Foundation

enum DeviceCode {
    static let length = 12

    static func symbols(in text: String) -> String {
        String(text.uppercased().filter { $0.isASCII && ($0.isLetter || $0.isNumber) }.prefix(length))
    }

    static func format(_ text: String) -> String {
        let symbols = Array(symbols(in: text))
        return stride(from: 0, to: symbols.count, by: 4)
            .map { String(symbols[$0 ..< min($0 + 4, symbols.count)]) }
            .joined(separator: "-")
    }

    static func isComplete(_ text: String) -> Bool {
        symbols(in: text).count == length
    }
}
