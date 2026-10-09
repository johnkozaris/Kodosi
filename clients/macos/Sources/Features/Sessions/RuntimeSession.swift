import Foundation

struct ProgramStatus: Decodable, Equatable, Sendable {
    enum State: String, Decodable, Sendable { case idle, working, done, blocked, error }
    enum Kind: String, Decodable, Sendable { case permission, question, auth }

    let state: State
    let kind: Kind?
    let progress: Int?
    let app: String?
    let title: String?
    let message: String?

    var caption: String? {
        let text = [title, message].compactMap(\.self).joined(separator: ": ")
        return text.isEmpty ? nil : text
    }
}

struct RuntimeSession: Decodable, Equatable, Identifiable, Sendable {
    enum Kind: String, Decodable, Sendable { case local, remote }
    enum Status: String, Decodable, Sendable { case running, reconnecting, closing }
    enum Connection: String, Decodable, Sendable { case local, connecting, connected, offline, blocked }

    let id: String
    let incarnationId: String
    let kind: Kind
    let program: String?
    let title: String?
    let programStatus: ProgramStatus?
    let name: String
    let workingDir: String?
    let ownerUserId: String?
    let ownerName: String?
    let hostDeviceId: String?
    let hostName: String?
    let isOwner: Bool
    let status: Status
    let connectionState: Connection
    let message: String?
    let missionId: String?
    let missionName: String?
    let connectedUsers: [String]?
    let sharedWith: [String]
    let createRequestId: String?

    var activity: String? {
        if let caption = programStatus?.caption {
            return caption
        }
        guard let title = TerminalSessionManager.sanitizeTerminalTitle(title) else { return nil }
        let trimmed = String(title.unicodeScalars.drop { Self.isStatusGlyph($0) || $0.properties.isWhitespace })
        return trimmed.isEmpty || trimmed == name ? nil : trimmed
    }

    var isWorking: Bool {
        guard canControl else { return false }
        if let programStatus {
            return programStatus.state == .working
        }
        guard let first = title?.unicodeScalars.first else { return false }
        return (0x2800 ... 0x28FF).contains(first.value)
    }

    var needsUser: Bool {
        canControl && programStatus?.state == .blocked
    }

    var waitState: ProgramStatus.State? {
        switch programStatus?.state {
        case .blocked: needsUser ? .blocked : nil
        case .done: .done
        case .error: .error
        default: nil
        }
    }

    var progress: Int? {
        isWorking ? programStatus?.progress : nil
    }

    var alert: String? {
        guard isOwner, waitState != nil else { return nil }
        return programStatus?.caption ?? sign(unseen: true)?.label
    }

    func mark(rested: AgentMark.Activity) -> AgentMark.Activity {
        if let progress {
            return .progress(progress)
        }
        if isWorking {
            return .working
        }
        return needsUser ? .asks : rested
    }

    func sign(unseen: Bool) -> StatusSign.Form? {
        switch waitState {
        case .blocked:
            switch programStatus?.kind {
            case .question: .question
            case .auth: .key
            case .permission, nil: .hand
            }
        case .done where unseen: .done
        case .error where unseen: .failed
        default: unseen ? .changed : nil
        }
    }

    var agent: AgentKind {
        AgentKind(program: program)
    }

    var isTroubled: Bool {
        connectionState == .blocked || (connectionState == .offline && (status == .reconnecting || message != nil))
    }

    var folderName: String? {
        workingDir.map { URL(fileURLWithPath: $0).lastPathComponent }
    }

    private static func isStatusGlyph(_ scalar: Unicode.Scalar) -> Bool {
        (0x2800 ... 0x28FF).contains(scalar.value) || [0x2733, 0x2722, 0x2736, 0x273B, 0x273D, 0x00B7, 0x25CF, 0x25CB].contains(scalar.value)
    }

    var isConnected: Bool {
        connectionState == .local || connectionState == .connected
    }

    var canControl: Bool {
        isConnected && status != .closing
    }

    var canOpen: Bool {
        status != .closing && connectionState != .blocked
    }

    var localDirectory: String? {
        kind == .local ? workingDir : nil
    }

    var hostLabel: String {
        kind == .local ? String(localized: "This Mac") : hostName ?? ownerName ?? String(localized: "Remote computer")
    }

    var statusLabel: String {
        switch connectionState {
        case .offline where status == .reconnecting: String(localized: "Its computer is offline")
        case .offline: String(localized: "Not connected")
        case .blocked: String(localized: "No access")
        case .connecting: String(localized: "Connecting")
        case .local, .connected:
            switch status {
            case .running: String(localized: "Running")
            case .reconnecting: String(localized: "Reconnecting")
            case .closing: String(localized: "Closing")
            }
        }
    }
}
