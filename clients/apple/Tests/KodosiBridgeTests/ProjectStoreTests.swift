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
        #expect(try await position(of: log.next("project.seen")) == 5)
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

    @Test func `an invite that the owner refused while making a project is not reported as missed`() async throws {
        var notices: [String] = []
        projects.notify = { notices.append($0) }
        let making = Task { try await projects.create(name: "Release", inviting: ["user-2"]) }
        let create = try await log.next("project.create")
        try reply(to: create, "project.result", ["operation": "project.create"])
        let invite = try await log.next("project.invite")
        try reply(to: invite, "project.error", [
            "operation": "project.invite", "message": "You did not allow this in Kodosi.", "refused": true,
        ])
        _ = try await making.value
        #expect(notices.isEmpty)
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

    func acted(_ action: String) -> [String: Any] {
        ["operation": "project.act", "projectId": "project-1", "itemId": NSNull(), "action": action]
    }

    func position(of command: [String: Any]) -> UInt64? {
        (command["position"] as? NSNumber)?.uint64Value
    }

    func content(_ id: String, messages: [[String: Any]]) -> [String: Any] {
        [
            "projectId": id, "messages": messages, "replacements": [], "clearedPosition": 0, "tasks": [],
            "repositories": [], "sequence": messages.count, "hasOlder": false, "historyTrimmed": false,
        ]
    }

    func message(_ id: String, _ position: Int, author: String) -> [String: Any] {
        [
            "id": id, "position": position, "authorId": author, "authorName": author == "user-1" ? "Me" : "Ana",
            "agent": NSNull(), "terminalId": NSNull(), "text": "Hello", "deleted": false,
            "createdAt": "2026-10-11T10:0\(position % 10):00Z",
        ]
    }
}
