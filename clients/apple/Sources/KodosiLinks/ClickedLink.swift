import Foundation
import KodosiKit
import UniformTypeIdentifiers

/// The runtime decides what a click on a link does, with the same rules in every app: web and
/// mail links open in this device's browser, files and folders open, links to other apps prompt
/// first, and malformed or disguised links don't open. A file or local address from a terminal on
/// another computer lives on that computer, so it does not open here. Whether opening a file runs
/// a program is left to this app, because Launch Services decides that.
public struct ClickedLink: Equatable, Sendable {
    public enum Decision: Equatable, Sendable {
        case open(URL)
        case ask(URL)
        case refuse(Refusal)
    }

    public enum Refusal: Equatable, Sendable {
        case malformed
        case hiddenCharacters
        case missingFile
        case program
        case fileElsewhere(String)
        case addressElsewhere(String)

        public var message: String {
            switch self {
            case .malformed:
                String(localized: "The link is not a complete address.", bundle: .module)
            case .hiddenCharacters:
                String(localized: "The link has hidden characters that can change how it looks.", bundle: .module)
            case .missingFile:
                String(localized: "The file or folder is not on this device.", bundle: .module)
            case .program:
                String(localized: "Opening it could run a program.", bundle: .module)
            case let .fileElsewhere(computer):
                String(localized: "The file is on \(computer).", bundle: .module)
            case let .addressElsewhere(computer):
                String(localized: "The address is on \(computer).", bundle: .module)
            }
        }
    }

    /// Sanitized display string with resolved local paths and hidden/control characters escaped.
    public let shown: String
    public let decision: Decision

    public init(_ value: String, otherComputer: String? = nil) {
        let decided = RuntimeDecision(value, otherComputer: otherComputer)
        shown = decided?.shown ?? value
        decision = decided.map(Self.decision) ?? .refuse(.malformed)
    }
}

private struct RuntimeDecision: Decodable {
    enum Action: String, Decodable {
        case open, ask, refuse
    }

    enum Refusal: String, Decodable {
        case malformed, hiddenCharacters, missingFile, program, fileElsewhere, addressElsewhere
    }

    let action: Action
    let target: String?
    let shown: String
    let refusal: Refusal?
    let place: String?

    init?(_ value: String, otherComputer: String?) {
        let link = Array(value.utf8)
        let computer = Array((otherComputer ?? "").utf8)
        var decision = [UInt8](repeating: 0, count: 4096)
        func decide() -> Int {
            Int(kodosi_link_decision(link, UInt(link.count), computer, UInt(computer.count), &decision, UInt(decision.count)))
        }
        var length = decide()
        if length > decision.count {
            decision = [UInt8](repeating: 0, count: length)
            length = decide()
        }
        guard (0 ... decision.count).contains(length),
              let decided = try? JSONDecoder().decode(Self.self, from: Data(decision.prefix(length)))
        else { return nil }
        self = decided
    }
}

extension ClickedLink {
    private static func decision(_ decided: RuntimeDecision) -> Decision {
        let target = decided.target.flatMap(URL.init(string:))
        switch (decided.action, target) {
        case let (.open, .some(url)):
            return url.isFileURL && runsCode(url) ? .refuse(.program) : .open(url)
        case let (.ask, .some(url)):
            return .ask(url)
        case (.open, .none), (.ask, .none):
            return .refuse(.malformed)
        case (.refuse, _):
            let place = decided.place ?? ""
            return switch decided.refusal {
            case .hiddenCharacters: .refuse(.hiddenCharacters)
            case .missingFile: .refuse(.missingFile)
            case .program: .refuse(.program)
            case .fileElsewhere: .refuse(.fileElsewhere(place))
            case .addressElsewhere: .refuse(.addressElsewhere(place))
            case .malformed, .none: .refuse(.malformed)
            }
        }
    }

    static func runsCode(_ file: URL) -> Bool {
        let file = file.standardizedFileURL.resolvingSymlinksInPath()
        guard let values = try? file.resourceValues(forKeys: [.contentTypeKey, .isDirectoryKey, .isExecutableKey])
        else { return true }
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

    /// Launch Services hands these to an app that runs them, whatever their content type.
    static let programExtensions: Set<String> = [
        "action", "app", "applescript", "class", "command", "desktop", "inetloc", "jar",
        "mobileconfig", "mpkg", "pkg", "scpt", "terminal", "tool", "url", "webloc", "workflow",
    ]
}
