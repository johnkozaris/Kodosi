import Foundation

public struct ProgramStatus: Decodable, Equatable, Sendable {
    public enum State: String, Decodable, Sendable { case idle, working, done, blocked, error }
    public enum Kind: String, Decodable, Sendable { case permission, question, auth }

    public let state: State
    public let kind: Kind?
    public let progress: Int?
    public let app: String?
    public let title: String?
    public let message: String?

    public var caption: String? {
        let text = [title, message].compactMap(\.self).joined(separator: ": ")
        return text.isEmpty ? nil : text
    }
}

public struct RuntimeSession: Decodable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Decodable, Sendable { case local, remote }
    public enum Status: String, Decodable, Sendable { case running, reconnecting, closing }
    public enum Connection: String, Decodable, Sendable { case local, connecting, connected, offline, blocked }

    public let id: String
    public let incarnationId: String
    public let kind: Kind
    public let program: String?
    public let prompt: Bool?
    public let repository: Bool?
    public let title: String?
    public let programStatus: ProgramStatus?
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
    public let missionId: String?
    public let missionName: String?
    public let connectedUsers: [String]?
    public let sharedWith: [String]
    public let createRequestId: String?

    public var activity: String? {
        if let caption = programStatus?.caption {
            return caption
        }
        guard let title = TerminalTitle.sanitize(title) else { return nil }
        let trimmed = String(title.unicodeScalars.drop { Self.isStatusGlyph($0) || $0.properties.isWhitespace })
        return trimmed.isEmpty || trimmed == name ? nil : trimmed
    }

    public var isWorking: Bool {
        guard canControl else { return false }
        if let programStatus {
            return programStatus.state == .working
        }
        guard let first = title?.unicodeScalars.first else { return false }
        return (0x2800 ... 0x28FF).contains(first.value)
    }

    public var atPrompt: Bool {
        kind == .local && canControl && prompt == true
    }

    public var needsUser: Bool {
        canControl && programStatus?.state == .blocked
    }

    public var waitState: ProgramStatus.State? {
        switch programStatus?.state {
        case .blocked: needsUser ? .blocked : nil
        case .done: .done
        case .error: .error
        default: nil
        }
    }

    public var progress: Int? {
        isWorking ? programStatus?.progress : nil
    }

    public var isTroubled: Bool {
        connectionState == .blocked || (connectionState == .offline && (status == .reconnecting || message != nil))
    }

    public var folderName: String? {
        workingDir.map(HostPath.folderName)
    }

    private static func isStatusGlyph(_ scalar: Unicode.Scalar) -> Bool {
        (0x2800 ... 0x28FF).contains(scalar.value) || [0x2733, 0x2722, 0x2736, 0x273B, 0x273D, 0x00B7, 0x25CF, 0x25CB].contains(scalar.value)
    }

    public var isConnected: Bool {
        connectionState == .local || connectionState == .connected
    }

    public var canControl: Bool {
        isConnected && status != .closing
    }

    public var canOpen: Bool {
        status != .closing && connectionState != .blocked
    }

    public var localDirectory: String? {
        kind == .local ? workingDir : nil
    }

    public var hostLabel: String {
        kind == .local ? String(localized: "This Mac", bundle: .module)
            : hostName ?? ownerName ?? String(localized: "Remote computer", bundle: .module)
    }

    public var statusLabel: String {
        switch connectionState {
        case .offline where status == .reconnecting: String(localized: "Its computer is offline", bundle: .module)
        case .offline: String(localized: "Not connected", bundle: .module)
        case .blocked: String(localized: "No access", bundle: .module)
        case .connecting: String(localized: "Connecting", bundle: .module)
        case .local, .connected:
            switch status {
            case .running: String(localized: "Running", bundle: .module)
            case .reconnecting: String(localized: "Reconnecting", bundle: .module)
            case .closing: String(localized: "Closing", bundle: .module)
            }
        }
    }
}
