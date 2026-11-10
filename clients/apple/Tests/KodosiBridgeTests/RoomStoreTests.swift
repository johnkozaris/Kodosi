import Foundation
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct RoomStoreTests {
    private let commands = CommandSink()
    private let rooms: RoomStore
    private let log = RoomCommandLog()

    init() {
        let log = log
        commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        commands.setAccount(userId: "user-1", epoch: 1)
        rooms = RoomStore(commands: commands)
        rooms.reset(userId: "user-1")
    }

    @Test func `list keeps rooms and invitations`() throws {
        try deliver("missions.snapshot", [
            "missions": [mission("room-1", owner: "user-1"), mission("room-2", owner: "user-2")],
            "invitations": [invitation("inv-1", room: "room-3")],
            "missionsTruncated": false, "invitationsTruncated": true,
        ])
        #expect(rooms.listed)
        #expect(rooms.missions.map(\.id) == ["room-1", "room-2"])
        #expect(rooms.invitations.map(\.missionName) == ["Launch"])
        #expect(rooms.listTruncated)
        #expect(rooms.isOwner(of: "room-1"))
        #expect(!rooms.isOwner(of: "room-2"))
    }

    @Test func `room that leaves the list is gone`() throws {
        try deliver("missions.snapshot", list(["room-1", "room-2"]))
        try deliver("room.snapshot", ["room": room("room-2", messages: [message("m1", 1, author: "user-2")])])
        try deliver("missions.snapshot", list([], truncated: true))
        #expect(rooms.departed.isEmpty)
        #expect(rooms.snapshots["room-2"] != nil)
        rooms.visibleRoomId = "room-3"
        try deliver("missions.snapshot", list([]))
        #expect(rooms.departed == ["room-1", "room-2"])
        #expect(rooms.snapshots["room-2"] == nil)
        try deliver("missions.snapshot", list(["room-1"]))
        #expect(rooms.departed == ["room-2"])
    }

    @Test func `later snapshots merge messages and count unread from others`() async throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2"), message("m2", 2, author: "user-1")])])
        try deliver("room.snapshot", ["room": room("room-1", messages: [
            message("m2", 2, author: "user-1"), message("m3", 3, author: "user-2"),
            message("m4", 4, author: "user-1"), message("m5", 5, author: "user-1", agent: "Claude Code"),
        ])])
        #expect(rooms.snapshots["room-1"]?.messages.map(\.id) == ["m1", "m2", "m3", "m4", "m5"])
        #expect(rooms.view("room-1").unread == 2)
        rooms.markRead("room-1")
        #expect(rooms.view("room-1").unread == 0)
        let seen = try await log.next("mission.seen")
        #expect(seen["missionId"] as? String == "room-1")
        #expect(sequence(of: seen) == 5)
    }

    @Test func `older pages are not new`() throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m5", 5, author: "user-2")], hasOlder: true)])
        try deliver("room.snapshot", ["room": room("room-1", messages: [
            message("m3", 3, author: "user-2"), message("m4", 4, author: "user-2"),
        ], hasOlder: false)])
        #expect(rooms.snapshots["room-1"]?.messages.map(\.id) == ["m3", "m4", "m5"])
        #expect(rooms.snapshots["room-1"]?.hasOlder == false)
        #expect(rooms.view("room-1").unread == 0)
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m6", 6, author: "user-2")], hasOlder: true)])
        #expect(rooms.snapshots["room-1"]?.hasOlder == false)
    }

    @Test func `reading the conversation keeps it read`() async throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        rooms.view("room-1").show(.conversation)
        #expect(try await sequence(of: log.next("mission.seen")) == 1)
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m2", 2, author: "user-2")])])
        #expect(rooms.view("room-1").unread == 0)
        #expect(try await sequence(of: log.next("mission.seen")) == 2)
        rooms.view("room-1").followsLatest = false
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m3", 3, author: "user-2")])])
        #expect(rooms.view("room-1").unread == 1)
        rooms.view("room-1").followsLatest = true
        #expect(rooms.view("room-1").unread == 0)
        #expect(try await sequence(of: log.next("mission.seen")) == 3)
    }

    @Test func `what the person read away from the front is seen when the app comes back`() async throws {
        var front = false
        rooms.isFront = { front }
        rooms.view("room-1").show(.conversation)
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m4", 4, author: "user-2")])])
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m5", 5, author: "user-2")])])
        #expect(rooms.view("room-1").unread == 0)
        rooms.markShownRead()
        #expect(log.take("mission.seen") == nil)
        front = true
        rooms.markShownRead()
        #expect(try await sequence(of: log.next("mission.seen")) == 5)
    }

    @Test func `a room that the list shows unread is seen again`() async throws {
        try deliver("missions.snapshot", list(["room-1"], unread: true))
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        rooms.markRead("room-1")
        rooms.markRead("room-1")
        #expect(try await sequence(of: log.next("mission.seen")) == 1)
        #expect(try await sequence(of: log.next("mission.seen")) == 1)
        try deliver("missions.snapshot", list(["room-1"]))
        rooms.markRead("room-1")
        #expect(log.take("mission.seen") == nil)
    }

    @Test func `tasks keep their newest version`() throws {
        try deliver("room.snapshot", ["room": room("room-1", tasks: [task("t1", version: 3, title: "New title")])])
        #expect(rooms.view("room-1").fresh.isEmpty)
        try deliver("room.snapshot", ["room": room("room-1", tasks: [
            task("t1", version: 2, title: "Old title"), task("t2", version: 1, title: "Second"),
        ])])
        #expect(rooms.snapshots["room-1"]?.tasks.map(\.title) == ["New title", "Second"])
        #expect(rooms.view("room-1").fresh == [.tasks])
        rooms.view("room-1").show(.tasks)
        #expect(rooms.view("room-1").fresh.isEmpty)
    }

    @Test func `issues are kept by repository`() throws {
        try deliver("room.result", [
            "requestId": "request-1", "operation": "room.command", "roomId": "room-1", "repositoryId": "repo-1",
            "issues": [[
                "repositoryId": "repo-1", "number": 12, "title": "Fix login", "body": "", "url": "https://github.com/team/app/issues/12",
                "closed": false, "assignees": ["ana"],
            ]],
        ])
        #expect(rooms.issues["repo-1"]?.map(\.number) == [12])
    }

    @Test func `account change clears rooms`() throws {
        try deliver("missions.snapshot", list(["room-1"]))
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        rooms.view("room-1").message = "Draft"
        rooms.reset(userId: "user-2")
        #expect(rooms.missions.isEmpty)
        #expect(rooms.snapshots.isEmpty)
        #expect(!rooms.listed)
        #expect(rooms.view("room-1").message.isEmpty)
    }

    @Test func `opening a room reads it and loads its members`() async throws {
        let opening = Task { await rooms.open("room-1") }
        let open = try await log.next("mission.open")
        #expect(fields(open) == ["missionId", "requestId"])
        #expect(open["missionId"] as? String == "room-1")
        let read = try await log.next("room.command")
        #expect(read["roomId"] as? String == "room-1")
        #expect(try action(read) as NSDictionary == ["type": "read"])
        try reply(to: open, "mission.result", [
            "operation": "mission.open",
            "mission": mission("room-1", owner: "user-2"),
            "members": [
                ["userId": "user-1", "handle": "me", "displayName": "Me", "isOwner": false],
                ["userId": "user-2", "handle": "ana", "displayName": "Ana", "isOwner": true],
            ],
        ])
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m1", 1, author: "user-2")])])
        try reply(to: read, "room.result", roomResult("read"))
        await opening.value
        #expect(rooms.details["room-1"]?.members.map(\.handle) == ["me", "ana"])
        #expect(rooms.snapshots["room-1"]?.messages.count == 1)
    }

    @Test func `the newest load of the people of a room wins`() async throws {
        rooms.visibleRoomId = "room-1"
        try deliver("missions.snapshot", list(["room-1"]))
        let first = try await log.next("mission.open")
        try deliver("missions.snapshot", list(["room-1"], truncated: true))
        let second = try await log.next("mission.open")
        try reply(to: first, "mission.result", ["operation": "mission.open", "mission": mission("room-1", owner: "user-1", name: "Before"), "members": []])
        try reply(to: second, "mission.result", ["operation": "mission.open", "mission": mission("room-1", owner: "user-1", name: "After"), "members": []])
        await RefreshWait.until { rooms.details["room-1"] != nil }
        #expect(rooms.details["room-1"]?.mission.name == "After")
        #expect(rooms.mission("room-1")?.name == "After")
    }

    @Test func `earlier messages ask before the first`() async throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [message("m42", 42, author: "user-2")], hasOlder: true)])
        let reading = Task { await rooms.read("room-1", before: 41) }
        let command = try await log.next("room.command")
        #expect(try action(command) as NSDictionary == ["type": "read", "before": 41])
        var page = roomResult("read")
        page["room"] = room("room-1", messages: [message("m40", 40, author: "user-2"), message("m41", 41, author: "user-2")])
        try reply(to: command, "room.result", page)
        await reading.value
        #expect(rooms.snapshots["room-1"]?.messages.map(\.id) == ["m40", "m41", "m42"])
        #expect(rooms.snapshots["room-1"]?.hasOlder == false)
        #expect(rooms.view("room-1").unread == 0)
    }

    @Test func `a read that stops is no failure`() async throws {
        let reading = Task { await rooms.read("room-1") }
        _ = try await log.next("room.command")
        reading.cancel()
        await reading.value
        #expect(rooms.view("room-1").readFailure == nil)
        #expect(!rooms.view("room-1").loading)
    }

    @Test func `post sends the trimmed draft and clears it`() async throws {
        let state = rooms.view("room-1")
        state.message = "  Hello @Ana  "
        state.followsLatest = false
        let posting = Task { await rooms.post(in: "room-1") }
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
        #expect(state.followsLatest)
    }

    @Test func `draft edited while sending stays`() async throws {
        let state = rooms.view("room-1")
        state.message = "First"
        let posting = Task { await rooms.post(in: "room-1") }
        let command = try await log.next("room.command")
        state.message = "First and more"
        try reply(to: command, "room.result", roomResult("post"))
        await posting.value
        #expect(state.message == "First and more")
    }

    @Test func `failed post keeps the draft`() async throws {
        let state = rooms.view("room-1")
        state.message = "Hello"
        let posting = Task { await rooms.post(in: "room-1") }
        let command = try await log.next("room.command")
        try reply(to: command, "room.error", ["operation": "room.command", "message": "Room text is empty or too long."])
        await posting.value
        #expect(state.failure == "Room text is empty or too long.")
        #expect(state.message == "Hello")
        #expect(state.sent == 0)
    }

    @Test func `create room uses its request as the room and invites friends`() async throws {
        let creating = Task { try await rooms.create(name: "  Launch week ", inviting: ["user-2", "user-3"]) }
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

    @Test func `invite names the room and the friend`() async throws {
        let inviting = Task { try await rooms.invite("user-2", to: "room-1") }
        let command = try await log.next("mission.invite")
        #expect(fields(command) == ["missionId", "requestId", "userId"])
        #expect(command["missionId"] as? String == "room-1")
        #expect(command["userId"] as? String == "user-2")
        try reply(to: command, "mission.error", ["operation": "mission.invite", "message": "Invite an accepted friend."])
        await #expect(throws: RuntimeError.operation("Invite an accepted friend.")) { try await inviting.value }
    }

    @Test(arguments: [true, false])
    func `answering an invitation names it`(accept: Bool) async throws {
        try deliver("missions.snapshot", [
            "missions": [], "invitations": [invitation("inv-1", room: "room-3")],
            "missionsTruncated": false, "invitationsTruncated": false,
        ])
        let invitation = try #require(rooms.invitations.first)
        let answering = Task { try await rooms.respond(to: invitation, accept: accept) }
        let type = accept ? "mission.invitation.accept" : "mission.invitation.reject"
        let command = try await log.next(type)
        #expect(fields(command) == ["invitationId", "requestId"])
        #expect(command["invitationId"] as? String == "inv-1")
        try reply(to: command, "mission.result", ["operation": type, "missionId": NSNull()])
        try await answering.value
    }

    @Test func `membership actions name the room`() async throws {
        try deliver("missions.snapshot", list(["room-1"]))
        let member = try JSONValue.object([
            "userId": .string("user-2"), "handle": .string("ana"), "displayName": .string("Ana"), "isOwner": .bool(false),
        ]).decode(as: MissionMemberEntry.self)
        let cases = [
            RoomMembershipCase(type: "mission.rename", fields: ["missionId", "name", "requestId"]) {
                try await rooms.rename("room-1", to: " Ship room ")
            },
            RoomMembershipCase(type: "mission.removeMember", fields: ["missionId", "requestId", "userId"]) {
                try await rooms.remove(member, from: "room-1")
            },
            RoomMembershipCase(type: "mission.leave", fields: ["missionId", "requestId"]) { try await rooms.leave("room-1") },
            RoomMembershipCase(type: "mission.delete", fields: ["missionId", "requestId"]) { try await rooms.delete("room-1") },
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

    @Test func `a room keeps its name unless the new one fits`() async throws {
        try deliver("missions.snapshot", list(["room-1"]))
        try await rooms.rename("room-1", to: "  ")
        try await rooms.rename("room-1", to: "Room room-1")
        await #expect(throws: RuntimeError.operation("Use a shorter name.")) {
            try await rooms.rename("room-1", to: String(repeating: "a", count: 129))
        }
        #expect(log.take("mission.rename") == nil)
    }

    @Test func `create task sends its fields`() async throws {
        let creating = Task {
            try await rooms.createTask(in: "room-1", title: " Ship it ", description: " Notes ", repositoryIds: ["repo-2", "repo-1"])
        }
        let command = try await log.next("room.command")
        #expect(try action(command) as NSDictionary == [
            "type": "createTask", "title": "Ship it", "description": "Notes", "repositoryIds": ["repo-1", "repo-2"],
        ])
        try reply(to: command, "room.result", roomResult("createTask"))
        try await creating.value
    }

    @Test func `message becomes a task`() async throws {
        try deliver("room.snapshot", ["room": room("room-1", messages: [
            message("m1", 1, author: "user-2", text: "\n  Fix the login page  \nIt fails on Safari."),
        ])])
        let message = try #require(rooms.snapshots["room-1"]?.messages.first)
        let making = Task { try await rooms.makeTask(from: message, in: "room-1") }
        let command = try await log.next("room.command")
        let made = try action(command)
        #expect(made["type"] as? String == "createTask")
        #expect(made["title"] as? String == "Fix the login page")
        #expect(made["description"] as? String == "From Ana: \n  Fix the login page  \nIt fails on Safari.")
        #expect(made["repositoryIds"] as? [String] == [])
        try reply(to: command, "room.result", roomResult("createTask"))
        try await making.value
    }

    @Test func `task changes send their fields`() async throws {
        try deliver("room.snapshot", ["room": room("room-1", tasks: [task("t1", version: 1, title: "Ship it")])])
        let task = try #require(rooms.snapshots["room-1"]?.tasks.first)
        let changes = [
            RoomTaskCase(change: .claim, note: nil, action: ["type": "updateTask", "taskId": "t1", "change": "claim"]),
            RoomTaskCase(change: .release, note: "  ", action: ["type": "updateTask", "taskId": "t1", "change": "release"]),
            RoomTaskCase(
                change: .close, note: " Merged #12 ", action: ["type": "updateTask", "taskId": "t1", "change": "close", "note": "Merged #12"],
            ),
            RoomTaskCase(change: .reopen, note: nil, action: ["type": "updateTask", "taskId": "t1", "change": "reopen"]),
        ]
        for item in changes {
            let updating = Task { try await rooms.update(task, in: "room-1", change: item.change, note: item.note) }
            let command = try await log.next("room.command")
            #expect(command["roomId"] as? String == "room-1")
            #expect(try action(command) as NSDictionary == item.action)
            try reply(to: command, "room.result", roomResult("updateTask"))
            try await updating.value
        }
    }

    @Test func `repository address is trimmed`() async throws {
        let adding = Task { try await rooms.addRepository(" https://github.com/team/app ", to: "room-1") }
        let command = try await log.next("room.command")
        #expect(try action(command) as NSDictionary == ["type": "addRepository", "url": "https://github.com/team/app"])
        try reply(to: command, "room.result", roomResult("addRepository"))
        try await adding.value
    }
}

private extension RoomStoreTests {
    func deliver(_ type: String, _ fields: [String: Any] = [:]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        let event = try JSONDecoder().decode(RuntimeEvent.self, from: data)
        commands.receive(event)
        try rooms.receive(event)
    }

    func reply(to command: [String: Any], _ type: String, _ fields: [String: Any]) throws {
        var payload = fields
        payload["requestId"] = command["requestId"]
        try deliver(type, payload)
    }

    func fields(_ command: [String: Any]) -> Set<String> {
        Set(command.keys).subtracting(["type", "accountUserId", "accountEpoch"])
    }

    func action(_ command: [String: Any]) throws -> [String: Any] {
        try #require(command["action"] as? [String: Any])
    }

    func roomResult(_ action: String) -> [String: Any] {
        ["operation": "room.command", "roomId": "room-1", "itemId": NSNull(), "action": action]
    }

    func sequence(of command: [String: Any]) -> UInt64? {
        (command["sequence"] as? NSNumber)?.uint64Value
    }

    func list(_ rooms: [String], truncated: Bool = false, unread: Bool = false) -> [String: Any] {
        [
            "missions": rooms.map { mission($0, owner: "user-1", unread: unread) }, "invitations": [],
            "missionsTruncated": truncated, "invitationsTruncated": false,
        ]
    }

    func mission(_ id: String, owner: String, name: String? = nil, unread: Bool = false) -> [String: Any] {
        ["id": id, "name": name ?? "Room \(id)", "ownerUserId": owner, "unread": unread]
    }

    func invitation(_ id: String, room: String) -> [String: Any] {
        ["id": id, "missionId": room, "missionName": "Launch", "inviterName": "Ana", "createdAt": "2026-10-11T10:00:00Z"]
    }

    func room(_ id: String, messages: [[String: Any]] = [], tasks: [[String: Any]] = [], hasOlder: Bool = false) -> [String: Any] {
        [
            "roomId": id, "messages": messages, "tasks": tasks, "repositories": [], "sequence": messages.count,
            "hasOlder": hasOlder, "historyTrimmed": false,
        ]
    }

    func message(_ id: String, _ sequence: Int, author: String, agent: String? = nil, text: String = "Hello") -> [String: Any] {
        [
            "id": id, "sequence": sequence, "authorId": author, "authorName": author == "user-1" ? "Me" : "Ana",
            "agent": agent ?? NSNull(), "terminalId": NSNull(), "text": text, "createdAt": "2026-10-11T10:0\(sequence % 10):00Z",
        ]
    }

    func task(_ id: String, version: Int, title: String) -> [String: Any] {
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
