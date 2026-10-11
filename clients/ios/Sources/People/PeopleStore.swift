import Foundation
import KodosiBridge
import Observation

@Observable
final class PeopleStore {
    private(set) var friends: [FriendEntry] = []
    private(set) var incoming: [FriendRequestEntry] = []
    private(set) var outgoing: [FriendRequestEntry] = []
    private(set) var ownInvite: String?
    private(set) var userId: String?

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private let notices: NoticeCenter

    init(commands: CommandSink, notices: NoticeCenter) {
        self.commands = commands
        self.notices = notices
    }

    func reset(userId: String?) {
        self.userId = userId
        friends = []
        incoming = []
        outgoing = []
        ownInvite = nil
    }

    func receive(_ event: RuntimeEvent) throws {
        if event.type == "friends.invite" {
            ownInvite = event.string("text")
        } else {
            friends = try event.value("friends")
            incoming = try event.value("incoming")
            outgoing = try event.value("outgoing")
        }
    }

    func name(of id: String) -> String? {
        friends.first { $0.userId == id }.map { $0.displayName.isEmpty ? $0.handle : $0.displayName }
    }
}
