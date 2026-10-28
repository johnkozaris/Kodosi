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
        let removal = String(localized: "It disconnects. Approve it again to use it. Kodosi does not undo the commands that it already ran.", bundle: .module)
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

public struct FolderList: Decodable, Equatable, Sendable {
    public let path: String
    public let parent: String?
    public let home: String
    public let folders: [String]
    public let recent: [String]

    public init(path: String, parent: String?, home: String, folders: [String], recent: [String]) {
        self.path = path
        self.parent = parent
        self.home = home
        self.folders = folders
        self.recent = recent
    }

    public var separator: String {
        home.contains("\\") ? "\\" : "/"
    }

    public func shown(_ folder: String) -> String {
        if folder == home {
            return "~"
        }
        let prefix = home.hasSuffix(separator) ? home : home + separator
        return folder.hasPrefix(prefix) ? "~" + separator + folder.dropFirst(prefix.count) : folder
    }

    public func child(_ name: String) -> String {
        path.hasSuffix(separator) ? path + name : path + separator + name
    }
}

public struct RepositoryChoices: Decodable, Equatable, Sendable {
    public enum GitHub: String, Decodable, Sendable {
        case ready, signedOut, missing
    }

    public let github: GitHub
    public let repositories: [RepositoryChoice]

    public init(github: GitHub, repositories: [RepositoryChoice]) {
        self.github = github
        self.repositories = repositories
    }
}

public struct RepositoryChoice: Decodable, Equatable, Identifiable, Sendable {
    public let name: String
    public let url: String
    public let provider: String
    public let folder: String?
    public let `private`: Bool

    public init(name: String, url: String, provider: String, folder: String? = nil, private: Bool = false) {
        self.name = name
        self.url = url
        self.provider = provider
        self.folder = folder
        self.private = `private`
    }

    public var id: String {
        url.lowercased()
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
