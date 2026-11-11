import Foundation
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct ProjectStoreTests {
    private let commands = CommandSink()
    private let projects: ProjectStore
    private let log = ProjectCommandLog()

    init() {
        let log = log
        commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        commands.setAccount(userId: "user-1", epoch: 1)
        projects = ProjectStore(commands: commands)
        projects.reset(userId: "user-1")
    }

    @Test func `list keeps projects and invitations`() throws {
        try deliver("projects.snapshot", [
            "projects": [project("project-1", owner: "user-1"), project("project-2", owner: "user-2")],
            "invitations": [invitation("inv-1", project: "project-3")],
            "projectsTruncated": false, "invitationsTruncated": true,
        ])
        #expect(projects.listed)
        #expect(projects.entries.map(\.id) == ["project-1", "project-2"])
        #expect(projects.invitations.map(\.projectName) == ["Launch"])
        #expect(projects.listTruncated)
        #expect(projects.isOwner(of: "project-1"))
        #expect(!projects.isOwner(of: "project-2"))
    }

    @Test func `project that leaves the list is gone`() throws {
        try deliver("projects.snapshot", list(["project-1", "project-2"]))
        try deliver("project.snapshot", ["content": content("project-2", messages: [message("m1", 1, author: "user-2")])])
        try deliver("projects.snapshot", list([], truncated: true))
        #expect(projects.departed.isEmpty)
        #expect(projects.snapshots["project-2"] != nil)
        projects.visibleProjectId = "project-3"
        try deliver("projects.snapshot", list([]))
        #expect(projects.departed == ["project-1", "project-2"])
        #expect(projects.snapshots["project-2"] == nil)
        try deliver("projects.snapshot", list(["project-1"]))
        #expect(projects.departed == ["project-2"])
    }

    @Test func `later snapshots merge messages and count unread from others`() async throws {
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m1", 1, author: "user-2"), message("m2", 2, author: "user-1")])])
        try deliver("project.snapshot", ["content": content("project-1", messages: [
            message("m2", 2, author: "user-1"), message("m3", 3, author: "user-2"),
            message("m4", 4, author: "user-1"), message("m5", 5, author: "user-1", agent: "Claude Code"),
        ])])
        #expect(projects.snapshots["project-1"]?.messages.map(\.id) == ["m1", "m2", "m3", "m4", "m5"])
        #expect(projects.view("project-1").unread == 2)
        projects.markRead("project-1")
        #expect(projects.view("project-1").unread == 0)
        let seen = try await log.next("project.seen")
        #expect(seen["projectId"] as? String == "project-1")
        #expect(sequence(of: seen) == 5)
    }

    @Test func `older pages are not new`() throws {
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m5", 5, author: "user-2")], hasOlder: true)])
        try deliver("project.snapshot", ["content": content("project-1", messages: [
            message("m3", 3, author: "user-2"), message("m4", 4, author: "user-2"),
        ], hasOlder: false)])
        #expect(projects.snapshots["project-1"]?.messages.map(\.id) == ["m3", "m4", "m5"])
        #expect(projects.snapshots["project-1"]?.hasOlder == false)
        #expect(projects.view("project-1").unread == 0)
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m6", 6, author: "user-2")], hasOlder: true)])
        #expect(projects.snapshots["project-1"]?.hasOlder == false)
    }

    @Test func `reading the conversation keeps it read`() async throws {
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m1", 1, author: "user-2")])])
        projects.view("project-1").show(.conversation)
        #expect(try await sequence(of: log.next("project.seen")) == 1)
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m2", 2, author: "user-2")])])
        #expect(projects.view("project-1").unread == 0)
        #expect(try await sequence(of: log.next("project.seen")) == 2)
        projects.view("project-1").followsLatest = false
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m3", 3, author: "user-2")])])
        #expect(projects.view("project-1").unread == 1)
        projects.view("project-1").followsLatest = true
        #expect(projects.view("project-1").unread == 0)
        #expect(try await sequence(of: log.next("project.seen")) == 3)
    }

    @Test func `what the person read away from the front is seen when the app comes back`() async throws {
        var front = false
        projects.isFront = { front }
        projects.view("project-1").show(.conversation)
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m4", 4, author: "user-2")])])
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m5", 5, author: "user-2")])])
        #expect(projects.view("project-1").unread == 0)
        projects.markShownRead()
        #expect(log.take("project.seen") == nil)
        front = true
        projects.markShownRead()
        #expect(try await sequence(of: log.next("project.seen")) == 5)
    }

    @Test func `a project that the list shows unread is seen again`() async throws {
        try deliver("projects.snapshot", list(["project-1"], unread: true))
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m1", 1, author: "user-2")])])
        projects.markRead("project-1")
        projects.markRead("project-1")
        #expect(try await sequence(of: log.next("project.seen")) == 1)
        #expect(try await sequence(of: log.next("project.seen")) == 1)
        try deliver("projects.snapshot", list(["project-1"]))
        projects.markRead("project-1")
        #expect(log.take("project.seen") == nil)
    }

    @Test func `tasks keep their newest version`() throws {
        try deliver("project.snapshot", ["content": content("project-1", tasks: [task("t1", version: 3, title: "New title")])])
        #expect(projects.view("project-1").fresh.isEmpty)
        try deliver("project.snapshot", ["content": content("project-1", tasks: [
            task("t1", version: 2, title: "Old title"), task("t2", version: 1, title: "Second"),
        ])])
        #expect(projects.snapshots["project-1"]?.tasks.map(\.title) == ["New title", "Second"])
        #expect(projects.view("project-1").fresh == [.tasks])
        projects.view("project-1").show(.tasks)
        #expect(projects.view("project-1").fresh.isEmpty)
    }

    @Test func `issues are kept by repository`() throws {
        try deliver("project.result", [
            "requestId": "request-1", "operation": "project.act", "projectId": "project-1", "repositoryId": "repo-1",
            "issues": [[
                "repositoryId": "repo-1", "number": 12, "title": "Fix login", "body": "", "url": "https://github.com/team/app/issues/12",
                "closed": false, "assignees": ["ana"],
            ]],
        ])
        #expect(projects.issues["repo-1"]?.map(\.number) == [12])
    }

    @Test func `account change clears projects`() throws {
        try deliver("projects.snapshot", list(["project-1"]))
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m1", 1, author: "user-2")])])
        projects.view("project-1").message = "Draft"
        projects.reset(userId: "user-2")
        #expect(projects.entries.isEmpty)
        #expect(projects.snapshots.isEmpty)
        #expect(!projects.listed)
        #expect(projects.view("project-1").message.isEmpty)
    }

    @Test func `opening a project reads it and loads its members`() async throws {
        let opening = Task { await projects.open("project-1") }
        let open = try await log.next("project.open")
        #expect(fields(open) == ["projectId", "requestId"])
        #expect(open["projectId"] as? String == "project-1")
        let read = try await log.next("project.act")
        #expect(read["projectId"] as? String == "project-1")
        #expect(try action(read) as NSDictionary == ["type": "read"])
        try reply(to: open, "project.result", [
            "operation": "project.open",
            "project": project("project-1", owner: "user-2"),
            "members": [
                ["userId": "user-1", "handle": "me", "displayName": "Me", "isOwner": false],
                ["userId": "user-2", "handle": "ana", "displayName": "Ana", "isOwner": true],
            ],
        ])
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m1", 1, author: "user-2")])])
        try reply(to: read, "project.result", acted("read"))
        await opening.value
        #expect(projects.details["project-1"]?.members.map(\.handle) == ["me", "ana"])
        #expect(projects.snapshots["project-1"]?.messages.count == 1)
    }

    @Test func `the newest load of the people of a project wins`() async throws {
        projects.visibleProjectId = "project-1"
        try deliver("projects.snapshot", list(["project-1"]))
        let first = try await log.next("project.open")
        try deliver("projects.snapshot", list(["project-1"], truncated: true))
        let second = try await log.next("project.open")
        try reply(to: first, "project.result", ["operation": "project.open", "project": project("project-1", owner: "user-1", name: "Before"), "members": []])
        try reply(to: second, "project.result", ["operation": "project.open", "project": project("project-1", owner: "user-1", name: "After"), "members": []])
        await RefreshWait.until { projects.details["project-1"] != nil }
        #expect(projects.details["project-1"]?.project.name == "After")
        #expect(projects.project("project-1")?.name == "After")
    }

    @Test func `earlier messages ask before the first`() async throws {
        try deliver("project.snapshot", ["content": content("project-1", messages: [message("m42", 42, author: "user-2")], hasOlder: true)])
        let reading = Task { await projects.read("project-1", before: 41) }
        let command = try await log.next("project.act")
        #expect(try action(command) as NSDictionary == ["type": "read", "before": 41])
        var page = acted("read")
        page["content"] = content("project-1", messages: [message("m40", 40, author: "user-2"), message("m41", 41, author: "user-2")])
        try reply(to: command, "project.result", page)
        await reading.value
        #expect(projects.snapshots["project-1"]?.messages.map(\.id) == ["m40", "m41", "m42"])
        #expect(projects.snapshots["project-1"]?.hasOlder == false)
        #expect(projects.view("project-1").unread == 0)
    }

    @Test func `a read that stops is no failure`() async throws {
        let reading = Task { await projects.read("project-1") }
        _ = try await log.next("project.act")
        reading.cancel()
        await reading.value
        #expect(projects.view("project-1").readFailure == nil)
        #expect(!projects.view("project-1").loading)
    }

    @Test func `post sends the trimmed draft and clears it`() async throws {
        let state = projects.view("project-1")
        state.message = "  Hello @Ana  "
        state.followsLatest = false
        let posting = Task { await projects.post(in: "project-1") }
        let command = try await log.next("project.act")
        #expect(fields(command) == ["action", "requestId", "projectId"])
        #expect(command["projectId"] as? String == "project-1")
        #expect(try action(command) as NSDictionary == ["type": "post", "text": "Hello @Ana"])
        #expect(state.sending)
        try reply(to: command, "project.result", acted("post"))
        await posting.value
        #expect(state.message.isEmpty)
        #expect(state.sent == 1)
        #expect(!state.sending)
        #expect(state.followsLatest)
    }

    @Test func `draft edited while sending stays`() async throws {
        let state = projects.view("project-1")
        state.message = "First"
        let posting = Task { await projects.post(in: "project-1") }
        let command = try await log.next("project.act")
        state.message = "First and more"
        try reply(to: command, "project.result", acted("post"))
        await posting.value
        #expect(state.message == "First and more")
    }

    @Test func `failed post keeps the draft`() async throws {
        let state = projects.view("project-1")
        state.message = "Hello"
        let posting = Task { await projects.post(in: "project-1") }
        let command = try await log.next("project.act")
        try reply(to: command, "project.error", ["operation": "project.act", "message": "The text is empty or too long."])
        await posting.value
        #expect(state.failure == "The text is empty or too long.")
        #expect(state.message == "Hello")
        #expect(state.sent == 0)
    }

    @Test func `create project uses its request as the project and invites friends`() async throws {
        let creating = Task { try await projects.create(name: "  Launch week ", inviting: ["user-2", "user-3"]) }
        let create = try await log.next("project.create")
        #expect(fields(create) == ["name", "requestId"])
        #expect(create["name"] as? String == "Launch week")
        let projectId = try #require(create["requestId"] as? String)
        try reply(to: create, "project.result", ["operation": "project.create", "projectId": NSNull()])
        for friend in ["user-2", "user-3"] {
            let invite = try await log.next("project.invite")
            #expect(fields(invite) == ["projectId", "requestId", "userId"])
            #expect(invite["projectId"] as? String == projectId)
            #expect(invite["userId"] as? String == friend)
            try reply(to: invite, "project.result", ["operation": "project.invite", "projectId": projectId])
        }
        #expect(try await creating.value == projectId)
    }

    @Test func `a project keeps its name unless the new one fits`() async throws {
        try deliver("projects.snapshot", list(["project-1"]))
        try await projects.rename("project-1", to: "  ")
        try await projects.rename("project-1", to: "Project project-1")
        await #expect(throws: RuntimeError.operation("Use a shorter name.")) {
            try await projects.rename("project-1", to: String(repeating: "a", count: 129))
        }
        #expect(log.take("project.rename") == nil)
    }

    @Test func `create task sends its fields`() async throws {
        let creating = Task {
            try await projects.createTask(in: "project-1", title: " Ship it ", description: " Notes ", repositoryIds: ["repo-2", "repo-1"])
        }
        let command = try await log.next("project.act")
        #expect(try action(command) as NSDictionary == [
            "type": "createTask", "title": "Ship it", "description": "Notes", "repositoryIds": ["repo-1", "repo-2"],
        ])
        try reply(to: command, "project.result", acted("createTask"))
        try await creating.value
    }

    @Test func `message becomes a task`() async throws {
        try deliver("project.snapshot", ["content": content("project-1", messages: [
            message("m1", 1, author: "user-2", text: "\n  Fix the login page  \nIt fails on Safari."),
        ])])
        let message = try #require(projects.snapshots["project-1"]?.messages.first)
        let making = Task { try await projects.makeTask(from: message, in: "project-1") }
        let command = try await log.next("project.act")
        let made = try action(command)
        #expect(made["type"] as? String == "createTask")
        #expect(made["title"] as? String == "Fix the login page")
        #expect(made["description"] as? String == "From Ana: \n  Fix the login page  \nIt fails on Safari.")
        #expect(made["repositoryIds"] as? [String] == [])
        try reply(to: command, "project.result", acted("createTask"))
        try await making.value
    }

    @Test func `repository address is trimmed`() async throws {
        let adding = Task { try await projects.addRepository(" https://github.com/team/app ", to: "project-1") }
        let command = try await log.next("project.act")
        #expect(try action(command) as NSDictionary == ["type": "addRepository", "url": "https://github.com/team/app"])
        try reply(to: command, "project.result", acted("addRepository"))
        try await adding.value
    }
}

private extension ProjectStoreTests {
    func deliver(_ type: String, _ fields: [String: Any] = [:]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        let event = try JSONDecoder().decode(RuntimeEvent.self, from: data)
        commands.receive(event)
        try projects.receive(event)
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

    func acted(_ action: String) -> [String: Any] {
        ["operation": "project.act", "projectId": "project-1", "itemId": NSNull(), "action": action]
    }

    func sequence(of command: [String: Any]) -> UInt64? {
        (command["sequence"] as? NSNumber)?.uint64Value
    }

    func list(_ projects: [String], truncated: Bool = false, unread: Bool = false) -> [String: Any] {
        [
            "projects": projects.map { project($0, owner: "user-1", unread: unread) }, "invitations": [],
            "projectsTruncated": truncated, "invitationsTruncated": false,
        ]
    }

    func project(_ id: String, owner: String, name: String? = nil, unread: Bool = false) -> [String: Any] {
        ["id": id, "name": name ?? "Project \(id)", "ownerUserId": owner, "unread": unread]
    }

    func invitation(_ id: String, project: String) -> [String: Any] {
        ["id": id, "projectId": project, "projectName": "Launch", "inviterName": "Ana", "createdAt": "2026-10-11T10:00:00Z"]
    }

    func content(_ id: String, messages: [[String: Any]] = [], tasks: [[String: Any]] = [], hasOlder: Bool = false) -> [String: Any] {
        [
            "projectId": id, "messages": messages, "tasks": tasks, "repositories": [], "sequence": messages.count,
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
