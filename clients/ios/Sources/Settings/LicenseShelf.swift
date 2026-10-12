import Foundation

nonisolated struct LicenseFile: Identifiable, Hashable, Sendable {
    let url: URL
    let title: String
    let folder: String?

    var id: URL {
        url
    }
}

nonisolated enum LicenseBlock: Hashable, Sendable {
    case heading(String)
    case text(AttributedString)
}

nonisolated enum LicenseShelf {
    static let folderName = "Notices"
    private static let textExtensions: Set<String> = ["", "txt", "md", "markdown", "text"]

    static var bundledFolder: URL? {
        Bundle.main.url(forResource: folderName, withExtension: nil)
    }

    @concurrent static func files(in root: URL?) async -> [LicenseFile] {
        guard let root, let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]
        ) else { return [] }
        let base = root.standardizedFileURL.pathComponents
        return enumerator.compactMap { $0 as? URL }
            .filter { url in
                (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true
                    && textExtensions.contains(url.pathExtension.lowercased())
            }
            .map { url in
                let parents = url.standardizedFileURL.deletingLastPathComponent().pathComponents.dropFirst(base.count)
                return LicenseFile(url: url, title: title(for: url), folder: parents.isEmpty ? nil : parents.joined(separator: "/"))
            }
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
    }

    @concurrent static func blocks(of file: LicenseFile) async -> [LicenseBlock]? {
        guard let data = try? Data(contentsOf: file.url), let text = String(data: data, encoding: .utf8) else { return nil }
        let markdown = ["md", "markdown"].contains(file.url.pathExtension.lowercased())
        return paragraphs(in: text).map { block($0, markdown: markdown) }
    }

    static func block(_ paragraph: String, markdown: Bool) -> LicenseBlock {
        guard markdown else { return .text(AttributedString(paragraph)) }
        if paragraph.hasPrefix("#"), !paragraph.contains("\n") {
            return .heading(String(paragraph.drop { $0 == "#" }).trimmingCharacters(in: .whitespaces))
        }
        let parsed = try? AttributedString(markdown: paragraph, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))
        return .text(parsed ?? AttributedString(paragraph))
    }

    static func title(for url: URL) -> String {
        let name = url.deletingPathExtension().lastPathComponent
            .replacingOccurrences(of: "_", with: " ")
            .replacingOccurrences(of: "-", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return name.isEmpty ? url.lastPathComponent : name
    }

    static func paragraphs(in text: String) -> [String] {
        text.replacingOccurrences(of: "\r\n", with: "\n")
            .components(separatedBy: "\n\n")
            .map(reflow)
            .filter { !$0.isEmpty }
    }

    private static func reflow(_ paragraph: String) -> String {
        let trimmed = paragraph.trimmingCharacters(in: .newlines)
        let lines = trimmed.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        guard lines.count > 1, lines.allSatisfy(isProse) else { return trimmed }
        return lines.map { $0.trimmingCharacters(in: .whitespaces) }.joined(separator: " ")
    }

    private static func isProse(_ line: String) -> Bool {
        guard let first = line.first, !first.isWhitespace else { return false }
        if "#*-+|>=".contains(first) {
            return false
        }
        let marker = line.prefix { $0.isNumber || $0 == "." || $0 == ")" }
        return marker.isEmpty || !marker.contains(where: { $0 == "." || $0 == ")" })
    }
}
