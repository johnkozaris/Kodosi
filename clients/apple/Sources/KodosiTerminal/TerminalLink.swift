import Foundation
import UniformTypeIdentifiers

/// A link that a person clicked in a terminal. Terminal output can come from any
/// text, so the link opens by Ghostty's rules: web and mail links open, a file or
/// folder opens unless it can run code, a link to another app asks first, and a
/// malformed or disguised link does not open. A file or a local address in a
/// terminal of another computer is on that computer, so it does not open here.
struct TerminalLink: Equatable {
    enum Decision: Equatable {
        case open(URL)
        case ask(URL)
        case refuse(Refusal)
    }

    enum Refusal: Equatable {
        case malformed
        case hiddenCharacters
        case noWebHost
        case missingFile
        case program
        case fileElsewhere(String)
        case addressElsewhere(String)

        var message: String {
            switch self {
            case .malformed:
                String(localized: "The link is not a complete address.")
            case .hiddenCharacters:
                String(localized: "The link has hidden characters that can change how it looks.")
            case .noWebHost:
                String(localized: "The web link has no host.")
            case .missingFile:
                String(localized: "The file or folder is not on this device.")
            case .program:
                String(localized: "Opening it could run a program.")
            case let .fileElsewhere(computer):
                String(localized: "The file is on \(computer).")
            case let .addressElsewhere(computer):
                String(localized: "The address is on \(computer).")
            }
        }
    }

    /// The target as it opens, on one line, with hidden characters written out.
    let shown: String
    let decision: Decision

    /// `otherComputer` names the computer that runs the terminal when it is not this
    /// device.
    init(_ value: String, otherComputer: String? = nil) {
        let target = otherComputer == nil ? Self.target(value) : value
        shown = Self.shown(target, resolvesFiles: otherComputer == nil)
        decision = Self.decision(value, target: target, otherComputer: otherComputer)
    }
}

private extension TerminalLink {
    /// Ghostty also finds paths in the text. A path opens as a file URL.
    static func target(_ value: String) -> String {
        guard URL(string: value)?.scheme == nil, !hasHiddenCharacters(value) else { return value }
        return URL(filePath: NSString(string: value).standardizingPath).absoluteString
    }

    static func decision(_ value: String, target: String, otherComputer: String?) -> Decision {
        guard !value.isEmpty else { return .refuse(.malformed) }
        guard !hasHiddenCharacters(value) else { return .refuse(.hiddenCharacters) }
        guard let url = URL(string: target), let scheme = url.scheme?.lowercased(), !scheme.isEmpty else {
            return otherComputer.map { .refuse(.fileElsewhere($0)) } ?? .refuse(.malformed)
        }
        switch scheme {
        case "http", "https":
            guard let host = url.host(), !host.isEmpty else { return .refuse(.noWebHost) }
            if let otherComputer, isLoopback(host) {
                return .refuse(.addressElsewhere(otherComputer))
            }
            return .open(url)
        case "mailto":
            guard let address = URLComponents(url: url, resolvingAgainstBaseURL: false)?.path,
                  !address.isEmpty
            else { return .refuse(.malformed) }
            return .open(url)
        case "file":
            return fileDecision(url, otherComputer: otherComputer)
        default:
            return .ask(url)
        }
    }

    static func fileDecision(_ url: URL, otherComputer: String?) -> Decision {
        guard url.isFileURL, url.query() == nil, url.fragment() == nil else { return .refuse(.malformed) }
        if let otherComputer {
            return .refuse(.fileElsewhere(otherComputer))
        }
        if let host = url.host(), !host.isEmpty, !isThisComputer(host) {
            return .refuse(.fileElsewhere(host))
        }
        let file = url.standardizedFileURL.resolvingSymlinksInPath()
        guard let values = try? file.resourceValues(forKeys: [
            .contentTypeKey, .isDirectoryKey, .isExecutableKey, .isRegularFileKey,
        ]), values.isDirectory == true || values.isRegularFile == true
        else { return .refuse(.missingFile) }
        guard !runsCode(file, values) else { return .refuse(.program) }
        return .open(file)
    }

    static func runsCode(_ file: URL, _ values: URLResourceValues) -> Bool {
        if programExtensions.contains(file.pathExtension.lowercased()) {
            return true
        }
        if let type = values.contentType,
           [UTType.application, .executable, .script].contains(where: { type.conforms(to: $0) })
        {
            return true
        }
        return values.isDirectory != true && values.isExecutable == true
    }

    /// Programs such as `ls --hyperlink` name the computer in a file link.
    static func isThisComputer(_ host: String) -> Bool {
        [thisComputer, "localhost"].contains { host.caseInsensitiveCompare($0) == .orderedSame }
    }

    static let thisComputer: String = {
        var name = [CChar](repeating: 0, count: 256)
        guard gethostname(&name, name.count) == 0 else { return "localhost" }
        return String(utf8String: name) ?? "localhost"
    }()

    /// An address on the loopback interface names the computer that serves it.
    static func isLoopback(_ host: String) -> Bool {
        let name = host.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        if ["localhost", "0.0.0.0", "::", "::1"].contains(name) || name.hasSuffix(".localhost") {
            return true
        }
        let parts = name.split(separator: ".", omittingEmptySubsequences: false)
        return parts.count == 4 && parts[0] == "127" && parts.allSatisfy { UInt8($0) != nil }
    }

    static func shown(_ target: String, resolvesFiles: Bool) -> String {
        let path = URL(string: target).flatMap { url -> String? in
            guard url.isFileURL else { return nil }
            return resolvesFiles
                ? url.standardizedFileURL.resolvingSymlinksInPath().path(percentEncoded: false)
                : url.path(percentEncoded: false)
        }
        return (path ?? target).unicodeScalars.map { scalar in
            isHidden(scalar) ? "\\u{\(String(scalar.value, radix: 16, uppercase: true))}" : String(scalar)
        }.joined()
    }

    static func hasHiddenCharacters(_ value: String) -> Bool {
        [value, value.removingPercentEncoding ?? ""].contains { text in
            text.unicodeScalars.contains(where: isHidden)
        }
    }

    /// Controls, line breaks, direction marks and invisible marks can make a link
    /// look like another one.
    static func isHidden(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.value {
        case 0x00 ... 0x1F, 0x7F ... 0x9F, 0x061C, 0x200B ... 0x200F, 0x202A ... 0x202E,
             0x2028 ... 0x2029, 0x2060, 0x2066 ... 0x2069, 0xFEFF:
            true
        default:
            false
        }
    }

    /// Launch Services opens these with an app that runs them, whatever their type.
    static let programExtensions: Set<String> = [
        "action", "app", "applescript", "class", "command", "desktop", "inetloc", "jar",
        "mobileconfig", "mpkg", "pkg", "scpt", "terminal", "tool", "url", "webloc", "workflow",
    ]
}
