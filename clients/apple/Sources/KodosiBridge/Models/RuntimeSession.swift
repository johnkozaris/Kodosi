import Foundation

public func refers(_ reference: String, to id: String, named name: String) -> Bool {
    guard !reference.isEmpty else { return false }
    return id == reference || (reference.count >= 4 && id.hasSuffix(reference))
        || name.caseInsensitiveCompare(reference) == .orderedSame
}

public extension [RuntimeSession] {
    func restart(of id: String, owner: String?, host: String?) -> RuntimeSession? {
        first { $0.restoredFrom == id && $0.ownerUserId == owner && $0.hostDeviceId == host }
    }
}

public func referred<Item>(_ reference: String, in items: [Item], id: KeyPath<Item, String>, name: KeyPath<Item, String>) -> Item? {
    guard !reference.isEmpty else { return nil }
    return items.first { $0[keyPath: id] == reference || $0[keyPath: name] == reference }
        ?? items.first { refers(reference, to: $0[keyPath: id], named: $0[keyPath: name]) }
}

public struct ProgramStatus: Decodable, Equatable, Sendable {
    public enum State: String, Decodable, Sendable { case idle, working, done, blocked, error }
    public enum Kind: String, Decodable, Sendable { case permission, question, auth }

    public let state: State
    public let kind: Kind?
    public let progress: Int?
    public let title: String?
    public let message: String?

    public var caption: String? {
        let text = [title, message].compactMap(\.self).joined(separator: ": ")
        return text.isEmpty ? nil : text
    }
}

public struct TerminalContext: Decodable, Equatable, Sendable {
    public let place: TerminalPlace?
    public let root: Bool
}

public struct TerminalPlace: Decodable, Equatable, Sendable {
    public enum Kind: String, Decodable, Sendable { case remote, container, vm }

    public let kind: Kind
    public let name: String
}

public struct RuntimeSession: Decodable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Decodable, Sendable { case local, remote }
    public enum Status: String, Decodable, Sendable { case running, reconnecting, offline, closing }
    public enum Connection: String, Decodable, Sendable { case local, connecting, connected, offline, blocked }
    public enum Waiting: String, Decodable, Sendable { case blocked, done, error }

    public let id: String
    public let incarnationId: String
    public let kind: Kind
    public let program: String?
    public let programAccount: Int?
    /// The agent in front started without Kodosi's note, because a person started it by hand.
    public let programOutside: Bool?
    public let programSettings: AgentSettings?
    /// What the agent in front can change while it runs.
    public let programLive: AgentLive?
    /// A change of the agent in front that waits for the end of its turn.
    public let programPending: AgentSettings?
    public let prompt: Bool?
    public let repository: Bool?
    public let programStatus: ProgramStatus?
    public let working: Bool
    public let waiting: Waiting?
    public let activity: String?
    public let context: TerminalContext?
    public let canvas: Canvas?
    public let name: String
    public let workingDir: String?
    public let ownerUserId: String?
    public let ownerName: String?
    public let hostDeviceId: String?
    public let hostName: String?
    public let isOwner: Bool
    public let status: Status
    public let connectionState: Connection
    public let message: String?
    public let projectId: String?
    public let projectName: String?
    public let connectedUsers: [String]?
    public let sharedWith: [String]
    public let restoredFrom: String?
    public let attention: Bool
    public let hidden: Bool

    public var atPrompt: Bool {
        kind == .local && canControl && prompt == true
    }

    public var needsUser: Bool {
        waiting == .blocked
    }

    public var progress: Int? {
        working ? programStatus?.progress : nil
    }

    public var place: TerminalPlace? {
        context?.place
    }

    public var runsAsRoot: Bool {
        context?.root == true
    }

    public var isTroubled: Bool {
        connectionState == .blocked || status == .offline || (connectionState == .offline && message != nil)
    }

    public var folderName: String? {
        workingDir.map(HostPath.folderName)
    }

    public var isConnected: Bool {
        connectionState == .local || connectionState == .connected
    }

    public var canControl: Bool {
        isConnected && status == .running
    }

    public var hostOffline: Bool {
        kind == .remote && status == .offline
    }

    public var canClose: Bool {
        canControl || hostOffline
    }

    public var canOpen: Bool {
        status != .closing && connectionState != .blocked
    }

    public var canCall: Bool {
        isOwner || isConnected
    }

    public var localDirectory: String? {
        kind == .local ? workingDir : nil
    }

    public var hostLabel: String {
        kind == .local ? String(localized: "This Mac", bundle: .module)
            : hostName ?? ownerName ?? String(localized: "Remote computer", bundle: .module)
    }

    public var otherComputer: String? {
        kind == .local ? nil : hostLabel
    }

    public var statusLabel: String {
        switch (connectionState, status) {
        case (.blocked, _): String(localized: "No access", bundle: .module)
        case (_, .offline): String(localized: "\(hostLabel) is offline", bundle: .module)
        case (.offline, _): String(localized: "Not connected", bundle: .module)
        case (.connecting, _): String(localized: "Connecting", bundle: .module)
        case (_, .running): String(localized: "Running", bundle: .module)
        case (_, .reconnecting): String(localized: "Reconnecting", bundle: .module)
        case (_, .closing): String(localized: "Closing", bundle: .module)
        }
    }
}
