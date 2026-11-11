import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct NewTerminalTests {
    private let model = AppModel()
    private let sent = SentCommands()
    private let defaults: UserDefaults

    init() throws {
        let sent = sent
        model.commands.transport = { data in
            RuntimeContract.verify(data)
            sent.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        defaults = try #require(UserDefaults(suiteName: "NewTerminalTests.\(UUID().uuidString)"))
        try deliver("auth.ready", ["userId": "user-1", "enrolled": true])
    }

    @Test func `a new terminal starts on the computer of its section, else the last one, else the first`() {
        let online = ["mini", "studio"]
        #expect(NewTerminal.computer(section: "studio", last: "mini", online: online) == "studio")
        #expect(NewTerminal.computer(section: "laptop", last: "mini", online: online) == "mini")
        #expect(NewTerminal.computer(section: nil, last: "laptop", online: online) == "mini")
        #expect(NewTerminal.computer(section: nil, last: nil, online: []) == nil)
    }

    @Test func `an agent starts on the other computer with its choices, its folder and the first prompt`() async throws {
        let studio = model.launcher(for: "studio")
        let draft = NewTerminal(request: NewTerminalRequest(), online: ["studio"], defaults: defaults)
        let refreshing = Task { await studio.refresh() }
        let agents = try await request("provider.agents")
        #expect(agents["computer"] as? String == "studio")
        try deliver("provider.result", ["requestId": agents["requestId"] ?? "", "operation": "provider.agents", "result": catalog])
        await refreshing.value
        draft.choose(folder: "/home/maya/atlas", in: studio)
        draft.prompt = "fix the failing test\n"
        let starting = Task { await draft.start(in: studio, on: model.terminals) }
        let create = try await request("session.create")
        #expect(create["computer"] as? String == "studio")
        #expect(create["workingDir"] as? String == "/home/maya/atlas")
        #expect(create["agent"] as? [String: String] == [
            "agent": "claude", "account": "work", "model": "", "effort": "high", "permissions": "unrestricted", "output": "quiet",
        ])
        #expect(create["prompt"] as? String == "fix the failing test")
        #expect(create["size"] == nil)
        #expect(create["projectId"] == nil)
        #expect((create["name"] as? String)?.isEmpty == false)
        try deliver("session.result", ["requestId": create["requestId"] ?? "", "operation": "session.create", "sessionId": "s1"])
        #expect(await starting.value == "s1")
        #expect(defaults.string(forKey: NewTerminal.lastComputerKey) == "studio")
    }

    @Test func `a shell starts in a project without a prompt, and a refusal keeps the computer's words`() async throws {
        let studio = model.launcher(for: "studio")
        let draft = NewTerminal(request: NewTerminalRequest(computer: "studio", projectId: "project"), online: ["studio"], defaults: defaults)
        draft.prompt = "ls"
        let starting = Task { await draft.start(in: studio, on: model.terminals) }
        let create = try await request("session.create")
        #expect(create["computer"] as? String == "studio")
        #expect(create["projectId"] as? String == "project")
        #expect(create["agent"] == nil)
        #expect(create["prompt"] == nil)
        #expect(create["workingDir"] == nil)
        try deliver("session.error", ["requestId": create["requestId"] ?? "", "operation": "session.create", "message": "There is no folder /gone."])
        #expect(await starting.value == nil)
        #expect(draft.failure == "There is no folder /gone.")
        #expect(defaults.string(forKey: NewTerminal.lastComputerKey) == nil)
    }

    @Test func `the plus of a folder starts the terminal in that folder of its computer`() async throws {
        let studio = model.launcher(for: "studio")
        let draft = NewTerminal(request: NewTerminalRequest(computer: "studio", folder: "/home/maya/atlas"), online: ["mini", "studio"],
                                defaults: defaults)
        #expect(draft.computer == "studio")
        #expect(draft.folder(in: studio) == "/home/maya/atlas")
        #expect(draft.folder(in: model.launcher(for: "mini")) == nil)
        let starting = Task { await draft.start(in: studio, on: model.terminals) }
        let create = try await request("session.create")
        #expect(create["computer"] as? String == "studio")
        #expect(create["workingDir"] as? String == "/home/maya/atlas")
        try deliver("session.result", ["requestId": create["requestId"] ?? "", "operation": "session.create", "sessionId": "s1"])
        #expect(await starting.value == "s1")
    }

    @Test func `the folders of another computer come from that computer`() async throws {
        let browser = FolderBrowser(launcher: model.launcher(for: "studio"))
        let opening = Task { await browser.open("/home/maya") }
        let list = try await request("folders.list")
        #expect(list["computer"] as? String == "studio")
        #expect(list["path"] as? String == "/home/maya")
        try deliver("folders.result", ["requestId": list["requestId"] ?? "", "operation": "folders.list", "folders": [
            "path": "/home/maya", "parent": "/home", "home": "/home/maya", "folders": ["atlas", "notes"], "recent": [String](),
        ]])
        await opening.value
        #expect(browser.listing?.folders == ["atlas", "notes"])
        #expect(browser.listing?.child("atlas") == "/home/maya/atlas")
        let gone = Task { await browser.open("/home/maya/atlas") }
        let again = try await request("folders.list", after: list)
        try deliver("folders.error", ["requestId": again["requestId"] ?? "", "operation": "folders.list", "message": "There is no folder /home/maya/atlas."])
        await gone.value
        #expect(browser.failure == "There is no folder /home/maya/atlas.")
        #expect(browser.listing?.path == "/home/maya")
    }

    @Test func `a folder page that goes away before its answer shows no error`() async throws {
        let browser = FolderBrowser(launcher: model.launcher(for: "studio"))
        let opening = Task { await browser.open("/home/maya") }
        let list = try await request("folders.list")
        opening.cancel()
        await opening.value
        let loading = Task { await browser.loadPlaces() }
        _ = try await request("folders.list", after: list)
        loading.cancel()
        await loading.value
        #expect(browser.failure == nil)
        #expect(browser.listing == nil)
    }

    private let catalog: [String: Any] = [
        "kodosi": NSNull(),
        "choice": [
            "agent": "claude",
            "picks": [["agent": "claude", "account": "work", "model": NSNull(), "effort": "high"]],
        ],
        "agents": [
            [
                "agent": "claude", "program": "claude", "installed": true, "install": "", "addsAccounts": true, "reportsUsage": true,
                "permissions": ["ask", "auto", "unrestricted"], "outputs": ["quiet", "standard", "detailed"],
                "defaultPermissions": "unrestricted", "defaultOutput": "quiet",
                "accounts": [
                    ["name": "default", "command": "claude", "identity": NSNull(), "folder": "~/.claude"],
                    ["name": "work", "command": "env CLAUDE_CONFIG_DIR=/home/maya/.claude-work claude", "identity": NSNull(),
                     "folder": "~/.claude-work"],
                ],
            ],
        ],
    ]

    private func request(_ type: String, after earlier: [String: Any]? = nil) async throws -> [String: Any] {
        let before = earlier?["requestId"] as? String
        var command: [String: Any]?
        for _ in 0 ..< 500 where command == nil {
            await Task.yield()
            command = sent.last(type).flatMap { $0["requestId"] as? String == before ? nil : $0 }
        }
        return try #require(command)
    }

    private func deliver(_ type: String, _ fields: [String: Any] = [:]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        try model.receive(JSONDecoder().decode(RuntimeEvent.self, from: data))
    }
}
