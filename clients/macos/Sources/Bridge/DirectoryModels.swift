import Foundation

struct FriendEntry: Decodable, Equatable, Identifiable, Sendable {
    let userId: String
    let handle: String
    let displayName: String
    let verified: Bool
    let identityState: String
    var id: String {
        userId
    }

    var identityChanged: Bool {
        identityState == "changed"
    }
}

struct FriendRequestEntry: Decodable, Equatable, Identifiable, Sendable {
    let userId: String
    let handle: String
    let displayName: String
    let createdAt: String
    var id: String {
        userId
    }
}

struct MyDeviceEntry: Decodable, Equatable, Identifiable, Sendable {
    let deviceId: String
    let label: String
    let certSignerDeviceId: String
    let certIssuedAtMs: UInt64
    let recoveryKey: Bool
    var id: String {
        deviceId
    }
}

struct DeviceLinkRequest: Decodable, Equatable, Identifiable, Sendable {
    let requestId: String
    let deviceLabel: String
    let expiresAt: String
    var id: String {
        requestId
    }
}

struct MissionEntry: Decodable, Equatable, Identifiable, Sendable {
    let id: String
    let name: String
    let ownerUserId: String
}

struct MissionMemberEntry: Decodable, Equatable, Identifiable, Sendable {
    let userId: String
    let handle: String
    let displayName: String
    let isOwner: Bool
    var id: String {
        userId
    }
}

struct MissionInvitationEntry: Decodable, Equatable, Identifiable, Sendable {
    let id: String
    let missionId: String
    let missionName: String
    let inviterName: String
    let createdAt: String
}

struct MissionDetail: Equatable {
    let mission: MissionEntry
    let members: [MissionMemberEntry]
}
