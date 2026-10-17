import Foundation

public struct FriendEntry: Decodable, Equatable, Identifiable, Sendable {
    public let userId: String
    public let handle: String
    public let displayName: String
    public let verified: Bool
    public let identityState: String
    public var id: String {
        userId
    }

    public var identityChanged: Bool {
        identityState == "changed"
    }

    public var isVerified: Bool {
        verified && !identityChanged
    }

    public var personName: String {
        displayName.isEmpty ? handle : displayName
    }
}

public struct FriendRequestEntry: Decodable, Equatable, Identifiable, Sendable {
    public let userId: String
    public let handle: String
    public let displayName: String
    public var id: String {
        userId
    }

    public var personName: String {
        displayName.isEmpty ? handle : displayName
    }
}

public struct MyDeviceEntry: Decodable, Equatable, Identifiable, Sendable {
    public let deviceId: String
    public let label: String
    public let certIssuedAtMs: UInt64
    public let recoveryKey: Bool
    public var id: String {
        deviceId
    }

    public var title: String {
        recoveryKey ? String(localized: "Recovery key", bundle: .module) : label
    }

    public func removalMessage(hasRecoveryKey: Bool) -> String {
        if recoveryKey {
            return String(localized: "The recovery key stops working. You can make a new one.", bundle: .module)
        }
        let removal = String(localized: "It disconnects. Approve it again to use it. Commands it already ran stay done.", bundle: .module)
        guard hasRecoveryKey else { return removal }
        return removal + " " + String(localized: "If it was lost or stolen, also make a new recovery key.", bundle: .module)
    }
}

public struct DeviceLinkRequest: Decodable, Equatable, Identifiable, Sendable {
    public let requestId: String
    public let deviceLabel: String
    public var id: String {
        requestId
    }
}

public struct MissionEntry: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let ownerUserId: String
    public let unread: Bool
}

public struct MissionMemberEntry: Decodable, Equatable, Identifiable, Sendable {
    public let userId: String
    public let handle: String
    public let displayName: String
    public let isOwner: Bool
    public var id: String {
        userId
    }

    public var personName: String {
        displayName.isEmpty ? handle : displayName
    }
}

public struct MissionInvitationEntry: Decodable, Equatable, Identifiable, Sendable {
    public let id: String
    public let missionId: String
    public let missionName: String
    public let inviterName: String
}

public struct MissionDetail: Equatable {
    public let mission: MissionEntry
    public let members: [MissionMemberEntry]

    public init(mission: MissionEntry, members: [MissionMemberEntry]) {
        self.mission = mission
        self.members = members
    }
}
