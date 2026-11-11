import Foundation

/// The history stays on this device.
public struct BrowserHistory: Equatable, Sendable {
    public static let storageKey = "browserHistory"
    private static let most = 300

    public struct Page: Codable, Equatable, Identifiable, Sendable {
        public let address: String
        public var title: String
        public var visits: Int
        public var last: Date

        public var id: String {
            address
        }
    }

    public private(set) var pages: [Page] = []

    public init() {}

    public mutating func visit(_ address: String, title: String, at time: Date = .now) {
        guard Self.kept(address) else { return }
        var page = pages.first { $0.address == address } ?? Page(address: address, title: title, visits: 0, last: time)
        pages.removeAll { $0.address == address }
        page.visits += 1
        page.last = time
        if !title.isEmpty {
            page.title = title
        }
        pages.insert(page, at: 0)
        if pages.count > Self.most {
            pages.removeLast(pages.count - Self.most)
        }
    }

    public mutating func name(_ address: String, title: String) {
        guard !title.isEmpty, let index = pages.firstIndex(where: { $0.address == address }) else { return }
        pages[index].title = title
    }

    public func matches(_ typed: String, most: Int = 6) -> [Page] {
        let start = Self.bare(typed.trimmingCharacters(in: .whitespaces))
        let words = start.split(whereSeparator: \.isWhitespace)
        guard !words.isEmpty else { return [] }
        return pages
            .filter { page in
                let text = Self.bare(page.address) + " " + page.title.lowercased()
                return words.allSatisfy { text.contains($0) }
            }
            .map { page in (page: page, starts: Self.bare(page.address).hasPrefix(start)) }
            .sorted { left, right in
                if left.starts != right.starts {
                    return left.starts
                }
                if left.page.visits != right.page.visits {
                    return left.page.visits > right.page.visits
                }
                return left.page.last > right.page.last
            }
            .prefix(most)
            .map(\.page)
    }

    public static func bare(_ address: String) -> String {
        var bare = address.lowercased()
        for prefix in ["https://", "http://", "www."] where bare.hasPrefix(prefix) {
            bare.removeFirst(prefix.count)
        }
        return bare
    }

    private static func kept(_ address: String) -> Bool {
        address.hasPrefix("https://") || address.hasPrefix("http://")
    }
}

extension BrowserHistory: Codable {
    public init(from decoder: any Decoder) throws {
        pages = try decoder.singleValueContainer().decode([Page].self)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(pages)
    }
}

/// `@AppStorage` keeps the history as JSON text. Two histories are the same by their pages, not
/// by that text.
extension BrowserHistory: RawRepresentable {
    // swiftformat:disable:next redundantEquatable
    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.pages == rhs.pages
    }

    public init?(rawValue: String) {
        guard let history = try? JSONDecoder().decode(Self.self, from: Data(rawValue.utf8)) else { return nil }
        self = history
    }

    public var rawValue: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        return (try? encoder.encode(self)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
    }
}
