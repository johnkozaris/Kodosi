import Foundation
import KodosiBridge
import Observation

@Observable
final class RoomStore {
    private(set) var missions: [MissionEntry] = []
    private(set) var invitations: [MissionInvitationEntry] = []
    private(set) var listTruncated = false
    private(set) var userId: String?

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private let notices: NoticeCenter

    init(commands: CommandSink, notices: NoticeCenter) {
        self.commands = commands
        self.notices = notices
    }

    func reset(userId: String?) {
        self.userId = userId
        missions = []
        invitations = []
        listTruncated = false
    }

    func receive(_ event: RuntimeEvent) throws {
        switch event.type {
        case "missions.snapshot":
            missions = try event.value("missions")
            invitations = try event.value("invitations")
            listTruncated = event.payload["missionsTruncated"] == .bool(true)
                || event.payload["invitationsTruncated"] == .bool(true)
        case "mission.result":
            commands.perform("mission.list")
        default:
            break
        }
    }
}
