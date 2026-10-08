import Foundation

struct RuntimeSession: Decodable, Equatable, Identifiable, Sendable {
    enum Kind: String, Decodable, Sendable { case local, remote }
    enum Status: String, Decodable, Sendable { case running, reconnecting, closing }
    enum Connection: String, Decodable, Sendable { case local, connecting, connected, offline, blocked }

    let id: String
    let incarnationId: String
    let kind: Kind
    let program: String?
    let title: String?
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

    var headerTitle: String {
        guard let title = TerminalSessionManager.sanitizeTerminalTitle(title), title != name else { return name }
        return "\(name) · \(title)"
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
        case .offline where status == .reconnecting: String(localized: "Host offline")
        case .offline: String(localized: "Not connected")
        case .blocked: String(localized: "Access unavailable")
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
