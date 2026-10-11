import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct RoomStoreTests {
    private let model = AppModel()
    private let log = RoomCommandLog()

    init() throws {
        let log = log
        model.commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
        log.clear()
    }

    @Test func listKeepsRoomsAndInvitations() throws {
        try deliver("missions.snapshot", [
            "missions": [mission("room-1", owner: "user-1"), mission("room-2", owner: "user-2")],
            "invitations": [invitation("inv-1", room: "room-3")],
            "missionsTruncated": false, "invitationsTruncated": true,
        ])
        #expect(model.rooms.listed)
        #expect(model.rooms.missions.map(\.id) == ["room-1", "room-2"])
        #expect(model.rooms.invitations.map(\.missionName) == ["Launch"])
        #expect(model.rooms.listTruncated)
        #expect(model.rooms.isOwner(of: "room-1"))
        #expect(!model.rooms.isOwner(of: "room-2"))
    }

    @Test func roomThatLeavesTheListIsGone() throws {
        try deliver("missions.snapshot", list(["room-1", "room-2"]))
        try deliver("room.snapshot", ["room": room("room-2", messages: [message("m1", 1, author: "user-2")])])
        model.rooms.visibleRoomId = "room-1"
        try deliver("missions.snapshot", list([]))
        #expect(model.rooms.departed == ["room-1"])
        #expect(model.rooms.snapshots["room-2"] == nil)
        try deliver("missions.snapshot", list(["room-1"]))
        #expect(model.rooms.departed.isEmpty)
    }

    @Test func laterSnapshotsMergeMessagesAndCountUnreadFromOthers() throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2"), message("m2", 2, author: "user-1")])])
        try deliver("room.snapshot", ["room": room("room-1", messages: [
            message("m2", 2, author: "user-1"), message("m3", 3, author: "user-2"),
            message("m4", 4, author: "user-1"), message("m5", 5, author: "user-1", agent: "Claude Code"),
        ])])
        #expect(model.rooms.snapshots["room-1"]?.messages.map(\.id) == ["m1", "m2", "m3", "m4", "m5"])
        #expect(model.rooms.unread["room-1"] == 2)
        model.rooms.markRead("room-1")
        #expect(model.rooms.unread["room-1"] == nil)
    }

    @Test func olderPagesAreNotNew() throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m5", 5, author: "user-2")], hasOlder: true)])
        try deliver("room.snapshot", ["room": room("room-1", messages: [
            message("m3", 3, author: "user-2"), message("m4", 4, author: "user-2"),
        ], hasOlder: false)])
        #expect(model.rooms.snapshots["room-1"]?.messages.map(\.id) == ["m3", "m4", "m5"])
        #expect(model.rooms.snapshots["room-1"]?.hasOlder == false)
        #expect(model.rooms.unread["room-1"] == nil)
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m6", 6, author: "user-2")], hasOlder: true)])
        #expect(model.rooms.snapshots["room-1"]?.hasOlder == false)
    }

    @Test func readingTheConversationKeepsItRead() throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        model.rooms.visibleRoomId = "room-1"
        model.rooms.view("room-1").watchingConversation = true
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m2", 2, author: "user-2")])])
        #expect(model.rooms.unread["room-1"] == nil)
        model.rooms.view("room-1").followsLatest = false
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m3", 3, author: "user-2")])])
        #expect(model.rooms.unread["room-1"] == 1)
    }

    @Test func tasksKeepTheirNewestVersion() throws {
        try deliver("room.snapshot", ["room": room("room-1", tasks: [task("t1", version: 3, title: "New title")])])
        #expect(!model.rooms.view("room-1").freshTasks)
        try deliver("room.snapshot", ["room": room("room-1", tasks: [
            task("t1", version: 2, title: "Old title"), task("t2", version: 1, title: "Second"),
        ])])
        #expect(model.rooms.snapshots["room-1"]?.tasks.map(\.title) == ["New title", "Second"])
        #expect(model.rooms.view("room-1").freshTasks)
        model.rooms.view("room-1").watchingTasks = true
        #expect(!model.rooms.view("room-1").freshTasks)
    }

    @Test func issuesAreKeptByRepository() throws {
        try deliver("room.issues", [
            "requestId": "request-1", "roomId": "room-1", "repositoryId": "repo-1",
            "issues": [[
                "repositoryId": "repo-1", "number": 12, "title": "Fix login", "body": "", "url": "https://github.com/team/app/issues/12",
                "closed": false, "assignees": ["ana"],
            ]],
        ])
        #expect(model.rooms.issues["repo-1"]?.map(\.number) == [12])
    }

    @Test func accountChangeClearsRooms() throws {
        try deliver("missions.snapshot", list(["room-1"]))
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        model.rooms.view("room-1").message = "Draft"
        model.rooms.reset(userId: "user-2")
        #expect(model.rooms.missions.isEmpty)
        #expect(model.rooms.snapshots.isEmpty)
        #expect(!model.rooms.listed)
        #expect(model.rooms.view("room-1").message.isEmpty)
    }

    @Test func openingARoomReadsItAndLoadsItsMembers() async throws {
        let opening = Task { await model.rooms.open("room-1") }
        let open = try await log.next("mission.open")
        #expect(fields(open) == ["missionId", "requestId"])
        #expect(open["missionId"] as? String == "room-1")
        let read = try await log.next("room.command")
        #expect(read["roomId"] as? String == "room-1")
        #expect(try action(read) as NSDictionary == ["type": "read"])
        try reply(to: open, "mission.snapshot", [
            "mission": mission("room-1", owner: "user-2"),
            "members": [
                ["userId": "user-1", "handle": "me", "displayName": "Me", "isOwner": false],
                ["userId": "user-2", "handle": "ana", "displayName": "Ana", "isOwner": true],
            ],
        ])
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        try reply(to: read, "room.result", roomResult("read"))
        await opening.value
        #expect(model.rooms.details["room-1"]?.members.map(\.handle) == ["me", "ana"])
        #expect(model.rooms.snapshots["room-1"]?.messages.count == 1)
    }

    @Test func earlierMessagesAskBeforeTheFirst() async throws {
        let reading = Task { await model.rooms.read("room-1", before: 41) }
        let command = try await log.next("room.command")
        #expect(try action(command) as NSDictionary == ["type": "read", "before": 41])
        try reply(to: command, "room.result", roomResult("read"))
        await reading.value
    }

    @Test func postSendsTheTrimmedDraftAndClearsIt() async throws {
        let state = model.rooms.view("room-1")
        state.message = "  Hello @Ana  "
        let posting = Task { await model.rooms.post(in: "room-1") }
        let command = try await log.next("room.command")
        #expect(fields(command) == ["action", "requestId", "roomId"])
        #expect(command["roomId"] as? String == "room-1")
        #expect(try action(command) as NSDictionary == ["type": "post", "text": "Hello @Ana"])
        #expect(state.sending)
        try reply(to: command, "room.result", roomResult("post"))
        await posting.value
        #expect(state.message.isEmpty)
        #expect(state.sent == 1)
        #expect(!state.sending)
    }

    @Test func draftEditedWhileSendingStays() async throws {
        let state = model.rooms.view("room-1")
        state.message = "First"
        let posting = Task { await model.rooms.post(in: "room-1") }
        let command = try await log.next("room.command")
        state.message = "First and more"
        try reply(to: command, "room.result", roomResult("post"))
        await posting.value
        #expect(state.message == "First and more")
    }

    @Test func failedPostKeepsTheDraft() async throws {
        let state = model.rooms.view("room-1")
        state.message = "Hello"
        let posting = Task { await model.rooms.post(in: "room-1") }
        let command = try await log.next("room.command")
        try reply(to: command, "room.error", ["operation": "room.command", "message": "Room text is empty or too long."])
        await posting.value
        #expect(state.failure == "Room text is empty or too long.")
        #expect(state.message == "Hello")
        #expect(state.sent == 0)
    }

    @Test func createRoomUsesItsRequestAsTheRoomAndInvitesFriends() async throws {
        let creating = Task { try await model.rooms.create(name: "  Launch week ", inviting: ["user-2", "user-3"]) }
        let create = try await log.next("mission.create")
        #expect(fields(create) == ["name", "requestId"])
        #expect(create["name"] as? String == "Launch week")
        let roomId = try #require(create["requestId"] as? String)
        try reply(to: create, "mission.result", ["operation": "mission.create", "missionId": NSNull()])
        for friend in ["user-2", "user-3"] {
            let invite = try await log.next("mission.invite")
            #expect(fields(invite) == ["missionId", "requestId", "userId"])
            #expect(invite["missionId"] as? String == roomId)
            #expect(invite["userId"] as? String == friend)
            try reply(to: invite, "mission.result", ["operation": "mission.invite", "missionId": roomId])
        }
        #expect(try await creating.value == roomId)
    }

    @Test func inviteNamesTheRoomAndTheFriend() async throws {
        let inviting = Task { try await model.rooms.invite("user-2", to: "room-1") }
        let command = try await log.next("mission.invite")
        #expect(fields(command) == ["missionId", "requestId", "userId"])
        #expect(command["missionId"] as? String == "room-1")
        #expect(command["userId"] as? String == "user-2")
        try reply(to: command, "mission.error", ["operation": "mission.invite", "message": "Invite an accepted friend."])
        await #expect(throws: RuntimeError.operation("Invite an accepted friend.")) { try await inviting.value }
    }

    @Test(arguments: [true, false])
    func answeringAnInvitationNamesIt(accept: Bool) async throws {
        try deliver("missions.snapshot", [
            "missions": [], "invitations": [invitation("inv-1", room: "room-3")],
            "missionsTruncated": false, "invitationsTruncated": false,
        ])
        let invitation = try #require(model.rooms.invitations.first)
        let answering = Task { try await model.rooms.respond(to: invitation, accept: accept) }
        let type = accept ? "mission.invitation.accept" : "mission.invitation.reject"
        let command = try await log.next(type)
        #expect(fields(command) == ["invitationId", "requestId"])
        #expect(command["invitationId"] as? String == "inv-1")
        try reply(to: command, "mission.result", ["operation": type, "missionId": NSNull()])
        try await answering.value
    }

    @Test func membershipActionsNameTheRoom() async throws {
        try deliver("missions.snapshot", list(["room-1"]))
        let member = try JSONValue.object([
            "userId": .string("user-2"), "handle": .string("ana"), "displayName": .string("Ana"), "isOwner": .bool(false),
        ]).decode(as: MissionMemberEntry.self)
        let cases = [
            RoomMembershipCase(type: "mission.rename", fields: ["missionId", "name", "requestId"]) {
                try await model.rooms.rename("room-1", to: " Ship room ")
            },
            RoomMembershipCase(type: "mission.removeMember", fields: ["missionId", "requestId", "userId"]) {
                try await model.rooms.remove(member, from: "room-1")
            },
            RoomMembershipCase(type: "mission.leave", fields: ["missionId", "requestId"]) { try await model.rooms.leave("room-1") },
            RoomMembershipCase(type: "mission.delete", fields: ["missionId", "requestId"]) { try await model.rooms.delete("room-1") },
        ]
        for item in cases {
            let running = Task { try await item.work() }
            let command = try await log.next(item.type)
            #expect(fields(command) == item.fields)
            #expect(command["missionId"] as? String == "room-1")
            try reply(to: command, "mission.result", ["operation": item.type, "missionId": "room-1"])
            try await running.value
        }
    }

    @Test func createTaskSendsItsFields() async throws {
        let creating = Task {
            try await model.rooms.createTask(in: "room-1", title: " Ship it ", description: " Notes ", repositoryIds: ["repo-2", "repo-1"])
        }
        let command = try await log.next("room.command")
        #expect(try action(command) as NSDictionary == [
            "type": "createTask", "title": "Ship it", "description": "Notes", "repositoryIds": ["repo-1", "repo-2"],
        ])
        try reply(to: command, "room.result", roomResult("createTask"))
        try await creating.value
    }

    @Test func messageBecomesATask() async throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [
            message("m1", 1, author: "user-2", text: "\n  Fix the login page  \nIt fails on Safari."),
        ])])
        let message = try #require(model.rooms.snapshots["room-1"]?.messages.first)
        let making = Task { try await model.rooms.makeTask(from: message, in: "room-1") }
        let command = try await log.next("room.command")
        let made = try action(command)
        #expect(made["type"] as? String == "createTask")
        #expect(made["title"] as? String == "Fix the login page")
        #expect(made["description"] as? String == "From Ana: \n  Fix the login page  \nIt fails on Safari.")
        #expect(made["repositoryIds"] as? [String] == [])
        try reply(to: command, "room.result", roomResult("createTask"))
        try await making.value
    }

    @Test func taskChangesSendTheirFields() async throws {
        try deliver("room.snapshot", ["room": room("room-1", tasks: [task("t1", version: 1, title: "Ship it")])])
        let task = try #require(model.rooms.snapshots["room-1"]?.tasks.first)
        let changes = [
            RoomTaskCase(change: .claim, note: nil, action: ["type": "updateTask", "taskId": "t1", "change": "claim"]),
            RoomTaskCase(change: .release, note: "  ", action: ["type": "updateTask", "taskId": "t1", "change": "release"]),
            RoomTaskCase(
                change: .close, note: " Merged #12 ", action: ["type": "updateTask", "taskId": "t1", "change": "close", "note": "Merged #12"]
            ),
            RoomTaskCase(change: .reopen, note: nil, action: ["type": "updateTask", "taskId": "t1", "change": "reopen"]),
        ]
        for item in changes {
            let updating = Task { try await model.rooms.update(task, in: "room-1", change: item.change, note: item.note) }
            let command = try await log.next("room.command")
            #expect(command["roomId"] as? String == "room-1")
            #expect(try action(command) as NSDictionary == item.action)
            try reply(to: command, "room.result", roomResult("updateTask"))
            try await updating.value
        }
    }

    @Test func repositoryAddressIsTrimmed() async throws {
        let adding = Task { try await model.rooms.addRepository(" https://github.com/team/app ", to: "room-1") }
        let command = try await log.next("room.command")
        #expect(try action(command) as NSDictionary == ["type": "addRepository", "url": "https://github.com/team/app"])
        try reply(to: command, "room.result", roomResult("addRepository"))
        try await adding.value
    }

    private func deliver(_ type: String, _ fields: [String: Any] = [:]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        try model.receive(JSONDecoder().decode(RuntimeEvent.self, from: data))
    }

    private func reply(to command: [String: Any], _ type: String, _ fields: [String: Any]) throws {
        var payload = fields
        payload["requestId"] = command["requestId"]
        try deliver(type, payload)
    }

    private func fields(_ command: [String: Any]) -> Set<String> {
        Set(command.keys).subtracting(["type", "accountUserId", "accountEpoch"])
    }

    private func action(_ command: [String: Any]) throws -> [String: Any] {
        try #require(command["action"] as? [String: Any])
    }

    private func roomResult(_ action: String) -> [String: Any] {
        ["operation": "room.command", "roomId": "room-1", "itemId": NSNull(), "action": action]
    }

    private func list(_ rooms: [String]) -> [String: Any] {
        [
            "missions": rooms.map { mission($0, owner: "user-1") }, "invitations": [],
            "missionsTruncated": false, "invitationsTruncated": false,
        ]
    }

    private func mission(_ id: String, owner: String) -> [String: Any] {
        ["id": id, "name": "Room \(id)", "ownerUserId": owner]
    }

    private func invitation(_ id: String, room: String) -> [String: Any] {
        ["id": id, "missionId": room, "missionName": "Launch", "inviterName": "Ana", "createdAt": "2026-10-11T10:00:00Z"]
    }

    private func room(_ id: String, messages: [[String: Any]] = [], tasks: [[String: Any]] = [], hasOlder: Bool = false) -> [String: Any] {
        [
            "roomId": id, "messages": messages, "tasks": tasks, "repositories": [], "sequence": messages.count,
            "hasOlder": hasOlder, "moreTasks": false,
        ]
    }

    private func message(_ id: String, _ sequence: Int, author: String, agent: String? = nil, text: String = "Hello") -> [String: Any] {
        [
            "id": id, "sequence": sequence, "authorId": author, "authorName": author == "user-1" ? "Me" : "Ana",
            "agent": agent ?? NSNull(), "terminalId": NSNull(), "text": text, "createdAt": "2026-10-11T10:0\(sequence % 10):00Z",
        ]
    }

    private func task(_ id: String, version: Int, title: String) -> [String: Any] {
        [
            "id": id, "version": version, "title": title, "description": "", "closed": false, "assignedTo": NSNull(),
            "assignedName": NSNull(), "terminalId": NSNull(), "repositoryIds": [], "note": NSNull(), "issue": NSNull(),
        ]
    }
}

struct RoomMembershipCase {
    let type: String
    let fields: Set<String>
    let work: () async throws -> Void
}

struct RoomTaskCase {
    let change: RoomTaskChange
    let note: String?
    let action: NSDictionary
}

enum RoomTestFailure: Error {
    case missing(String)
}

final class RoomCommandLog: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [[String: Any]] = []

    func append(_ data: Data) {
        guard let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        lock.withLock { values.append(value) }
    }

    func clear() {
        lock.withLock { values.removeAll() }
    }

    func next(_ type: String) async throws -> [String: Any] {
        for _ in 0 ..< 500 {
            if let found = take(type) {
                return found
            }
            await Task.yield()
        }
        throw RoomTestFailure.missing(type)
    }

    private func take(_ type: String) -> [String: Any]? {
        lock.withLock {
            guard let index = values.firstIndex(where: { $0["type"] as? String == type }) else { return nil }
            return values.remove(at: index)
        }
    }
}
