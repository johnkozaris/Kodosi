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

    func notificationTarget(_ id: String) -> RuntimeSession? {
        let listed = filter { !$0.hidden }
        return listed.first { $0.id == id } ?? listed.first { $0.restoredFrom == id && $0.isOwner }
    }
}

public func referred<Item>(_ reference: String, in items: [Item], id: KeyPath<Item, String>, name: KeyPath<Item, String>) -> Item? {
    guard !reference.isEmpty else { return nil }
    return items.first { $0[keyPath: id] == reference || $0[keyPath: name] == reference }
        ?? items.first { refers(reference, to: $0[keyPath: id], named: $0[keyPath: name]) }
}

public struct ProgramStatus: Decodable, Equatable, Sendable {
    public enum Kind: String, Decodable, Sendable { case permission, question, auth }

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

/// The look of the terminals of a computer. Its programs pick their colors from it, so every
/// device draws the computer's terminals in it.
public enum TerminalLook: String, Decodable, Sendable {
    case dark, light
}

/// The two faces of a terminal: its screen, and its chat.
public enum TerminalFace: String, Decodable, Hashable, Sendable {
    case terminal, chat
}

/// The face that a person picked for a terminal. It holds until the terminal's agent reports another
/// face, so a terminal whose agent goes to the chat or waits for its sign-in shows its own face.
public struct FacePick: Equatable, Sendable {
    public let face: TerminalFace
    public let over: TerminalFace?

    public init(_ face: TerminalFace, over: TerminalFace?) {
        self.face = face
        self.over = over
    }
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
    public let programNote: String?
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
    /// Where the agent that reports to the terminal's chat works, while one does.
    public let face: TerminalFace?
    private let helpers: Int?
    private let look: TerminalLook?
    public let name: String
    public let workingDir: String?
    public let ownerUserId: String?
    public let ownerName: String?
    public let hostDeviceId: String?
    public let hostName: String?
    public let isOwner: Bool
    public let status: Status
    public let connectionState: Connection
    /// This device's typing waits, and the connection to the other computer has not answered
    /// for a while. The connection stays open, and the typing arrives in order once it answers.
    public let stalled: Bool?
    /// This device's connection to the terminal's computer goes straight between the two devices,
    /// not through the service.
    public let direct: Bool?
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

    /// The face that the apps show: the person's pick while it holds, else where the agent works.
    public func shownFace(_ pick: FacePick?) -> TerminalFace {
        pick?.face ?? face ?? .terminal
    }

    public func keeps(_ pick: FacePick) -> Bool {
        face == nil || pick.over == face
    }

    public var terminalLook: TerminalLook {
        look ?? .dark
    }

    /// The helpers of the terminal's agent that work now.
    public var workingHelpers: Int {
        helpers ?? 0
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

    /// An open terminal of another computer that reconnects, or whose typing waits for its
    /// connection. Its screen stays, and typing goes on while its connection stalls.
    public var waitsForConnection: Bool {
        kind == .remote && isConnected && (status == .reconnecting || stalled == true)
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

    /// Only its owner's devices see a computer's screen, and the computer itself has no need to.
    public var showsScreen: Bool {
        isOwner && kind == .remote
    }

    public var pathLabel: String? {
        direct == true ? String(localized: "Straight connection", bundle: .module) : nil
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
