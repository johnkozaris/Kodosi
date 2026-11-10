import Foundation
import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing

func runtimeEvent(_ type: String, epoch: UInt64 = 1, userId: String? = "owner", fields: [String: JSONValue] = [:]) throws -> RuntimeEvent {
    var value = fields
    value["type"] = .string(type)
    value["accountEpoch"] = .uint(epoch)
    value["accountUserId"] = userId.map(JSONValue.string) ?? .null
    return try JSONDecoder().decode(RuntimeEvent.self, from: JSONEncoder().encode(value))
}

func runtimeSession(_ id: String = "01992e43-53db-7040-8e02-f6ebf4149b28", kind: String = "remote",
                    connection: String = "offline", status: String = "running", owner: Bool = true,
                    directory: String? = "/remote/project", restoredFrom: String? = nil,
                    ownerUserId: String? = nil, hostDeviceId: String? = nil, ownerName: String? = nil,
                    hostName: String? = nil) throws -> RuntimeSession
{
    var fields: [String: JSONValue] = [
        "id": .string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
        "kind": .string(kind), "name": .string("Terminal"),
        "isOwner": .bool(owner), "status": .string(status), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
        "connectionState": .string(connection),
        "sharedWith": .array([]),
    ]
    fields["workingDir"] = directory.map(JSONValue.string)
    fields["ownerName"] = ownerName.map(JSONValue.string)
    fields["restoredFrom"] = restoredFrom.map(JSONValue.string)
    fields["ownerUserId"] = ownerUserId.map(JSONValue.string)
    fields["hostDeviceId"] = hostDeviceId.map(JSONValue.string)
    fields["hostName"] = hostName.map(JSONValue.string)
    return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
}

@Test func `event envelope requires explicit account and exact epoch`() throws {
    let event = try runtimeEvent("system.ready", epoch: UInt64.max, userId: nil)
    #expect(event.accountEpoch == UInt64.max)
    #expect(event.accountUserId == nil)
    for source in [
        #"{"type":"system.ready","accountEpoch":1}"#,
        #"{"type":"system.ready","accountEpoch":-1,"accountUserId":null}"#,
        #"{"type":"system.ready","accountEpoch":true,"accountUserId":null}"#,
        #"{"type":"system.ready","accountEpoch":1.5,"accountUserId":null}"#,
        #"{"type":"system.ready","accountEpoch":1,"accountUserId":false}"#,
    ] {
        #expect(throws: (any Error).self) { try JSONDecoder().decode(RuntimeEvent.self, from: Data(source.utf8)) }
    }
}

@Test func `envelope cannot be overridden by command fields`() throws {
    let command = RuntimeCommand(type: "session.list", fields: ["accountEpoch": .int(99), "accountUserId": .string("wrong")])
    let data = try JSONEncoder().encode(RuntimeCommandEnvelope(accountUserId: nil, accountEpoch: UInt64.max, command: command))
    let decoded = try JSONDecoder().decode(RuntimeEvent.self, from: data)
    #expect(decoded.accountEpoch == UInt64.max)
    #expect(decoded.accountUserId == nil)
    #expect(decoded.type == "session.list")
}

@Test func `remote session never treats host directory as local path`() throws {
    let remote = try runtimeSession(connection: "connected", owner: false)
    #expect(remote.localDirectory == nil)
    #expect(remote.canControl)
    #expect(remote.canOpen)
    let local = try runtimeSession(kind: "local", connection: "local")
    #expect(local.localDirectory == "/remote/project")
    #expect(local.canControl)
    #expect(try !runtimeSession(connection: "blocked").canControl)
}

@Test func `a remote terminal takes no input while its computer is away`() throws {
    for status in ["reconnecting", "offline"] {
        let session = try runtimeSession(connection: "connected", status: status)
        #expect(session.isConnected)
        #expect(!session.canControl)
        #expect(session.canOpen)
    }
    let away = try runtimeSession(connection: "connected", status: "offline")
    #expect(away.isTroubled)
    #expect(away.statusLabel == "Remote computer is offline")
}

@Test func `deep links only identify real session UUIDs`() throws {
    let id = "01992e43-53db-7040-8e02-f6ebf4149b28"
    let url = try #require(URL(string: "kodosi://session/\(id)"))
    #expect(DeepLinkRouter.destination(for: url)?.sessionId == id)
    for source in [
        "kodosi://session/not-a-uuid", "kodosi://session/\(id)/extra", "kodosi://session/\(id)?toolUseId=old",
        "kodosi://session/\(id)#approval", "https://session/\(id)", "kodosi://user@session/\(id)",
    ] {
        #expect(try DeepLinkRouter.destination(for: #require(URL(string: source))) == nil)
    }
}

@Test func `event inbox is bounded ordered and closes waiter`() async {
    let inbox = RuntimeEventInbox()
    for index in 0 ..< 256 {
        #expect(inbox.push(Data("\(index)".utf8)))
    }
    #expect(!inbox.push(Data("overflow".utf8)))
    for index in 0 ..< 256 {
        #expect(await inbox.next() == Data("\(index)".utf8))
    }
    let pending = Task { await inbox.next() }
    inbox.close()
    #expect(await pending.value == nil)
    #expect(!inbox.push(Data()))
    inbox.reopen()
    #expect(inbox.push(Data("restarted".utf8)))
    #expect(await inbox.next() == Data("restarted".utf8))
    inbox.close(overflowed: true)
    #expect(inbox.didOverflow)
}

@Test func `terminal control inbox enforces its own count and byte budget`() async {
    let inbox = RuntimeEventInbox(maximumBytes: 8, maximumCount: 2)
    #expect(inbox.push(Data([1, 2, 3, 4])))
    #expect(inbox.push(Data([5, 6, 7, 8])))
    #expect(!inbox.push(Data([9])))
    #expect(await inbox.next() == Data([1, 2, 3, 4]))
    #expect(!inbox.push(Data(repeating: 0, count: 5)))
    #expect(await inbox.next() == Data([5, 6, 7, 8]))
    #expect(!inbox.push(Data(repeating: 0, count: 9)))
    inbox.close()
}

@Test @MainActor func `folder groups and navigation follow the working session`() throws {
    let first = try runtimeSession(kind: "local", connection: "local")
    let second = try runtimeSession("01992e43-53db-7040-8e02-f6ebf4149b30", kind: "local", connection: "local")
    let remote = try runtimeSession("01992e43-53db-7040-8e02-f6ebf4149b31")
    let computers = SessionComputerGroup.groups([first, second, remote])
    #expect(computers.count == 2)
    #expect(computers[0].folders.flatMap(\.sessions).map(\.id) == [first.id, second.id])
    #expect(computers[1].computer != nil && computers[1].owner == nil)
    let defaults = try #require(EphemeralUserDefaults(prefix: "FolderNavigation"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.showSession(first.id)
    workbench.showSession(second.id)
    workbench.selectAdjacentSession(offset: 1)
    #expect(workbench.selectedSessionId == first.id)
    workbench.sidebarCollapsed = true
    #expect(workbench.stagedSessionIds == [first.id, second.id])
}

@Test func `terminals group by computer, then by folder`() throws {
    let here = try runtimeSession(kind: "local", connection: "local", directory: "/Users/ana/kodosi")
    let studio = try runtimeSession("01992e43-53db-7040-8e02-f6ebf4149b31", directory: "/Users/ana/website",
                                    hostDeviceId: "studio", hostName: "Studio")
    let unopened = try runtimeSession("01992e43-53db-7040-8e02-f6ebf4149b32", owner: false, directory: nil,
                                      ownerUserId: "ben", hostDeviceId: "bens-mac", ownerName: "Ben Okafor", hostName: "Ben's Mac")
    let laptop = try runtimeSession("01992e43-53db-7040-8e02-f6ebf4149b33", owner: false, directory: "/home/ben/api",
                                    ownerUserId: "ben", hostDeviceId: "bens-laptop", ownerName: "Ben Okafor", hostName: "Laptop")

    let computers = SessionComputerGroup.groups([laptop, unopened, studio, here])

    #expect(computers.map(\.title) == ["This Mac", "Studio", "Ben Okafor · Ben's Mac", "Ben Okafor · Laptop"])
    #expect(computers[1].folders.map(\.computerId) == ["studio"])
    #expect(computers.map { $0.folders.map(\.canStartTerminal) } == [[true], [true], [false], [false]])
    #expect(computers[2].owner == SessionFolderGroup.Owner(name: "Ben Okafor", userId: "ben"))
    #expect(computers[2].folders.map(\.directory) == [nil])
}

@Test func `each host folder shows its last folder`() throws {
    for (path, name) in [
        ("/Users/ada/project", "project"), ("/home/ada/project/", "project"), (#"/home/ada/one\two"#, #"one\two"#), ("/", "/"),
        (#"C:\Users\ada\project"#, "project"), (#"d:/Users/ada/project\"#, "project"), (#"C:\"#, "C:"),
        (#"\\server\share\project"#, "project"), (#"\\server\share"#, "share"),
    ] {
        #expect(HostPath.folderName(path) == name)
    }
    let windows = try runtimeSession(directory: #"C:\Users\ada\project"#)
    #expect(windows.folderName == "project")
    #expect(SessionComputerGroup.groups([windows]).flatMap(\.folders).map(\.name) == ["project"])
}

@Test func `a terminal shows the state that the runtime gives`() throws {
    func session(
        working: Bool = false, waiting: String? = nil, activity: String? = nil, status: [String: JSONValue]? = nil,
        kind: String = "local", connection: String = "local",
    ) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string(kind), "name": .string("Amber Wren"), "program": .string("claude"),
            "isOwner": .bool(kind == "local"), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
            "connectionState": .string(connection), "sharedWith": .array([]), "working": .bool(working),
        ]
        fields["waiting"] = waiting.map(JSONValue.string)
        fields["activity"] = activity.map(JSONValue.string)
        fields["programStatus"] = status.map(JSONValue.object)
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
    }
    let working = try session(working: true, activity: "Draft the release notes")
    #expect(working.name == "Amber Wren")
    #expect(working.activity == "Draft the release notes")
    #expect(working.agent == .claude)
    #expect(working.mark(rested: .asleep) == .working)
    #expect(working.sign(unseen: false) == nil)
    let resting = try session()
    #expect(resting.activity == nil)
    #expect(resting.mark(rested: .asleep) == .asleep)
    #expect(resting.sign(unseen: false) == nil)
    #expect(resting.sign(unseen: true) == .changed)
    let measured = try session(working: true, status: ["state": .string("working"), "progress": .int(40)])
    #expect(!measured.needsUser)
    #expect(measured.progress == 40)
    #expect(measured.mark(rested: .asleep) == .progress(40))
    let approval = try session(
        waiting: "blocked", activity: "Allow the command?",
        status: ["state": .string("blocked"), "kind": .string("permission"), "message": .string("Allow the command?")],
    )
    #expect(approval.needsUser)
    #expect(approval.mark(rested: .asleep) == .asleep)
    #expect(approval.sign(unseen: false) == .hand)
    #expect(approval.alert == "Allow the command?")
    #expect(try session(waiting: "blocked", status: ["state": .string("blocked"), "kind": .string("question")]).sign(unseen: true) == .question)
    #expect(try session(waiting: "blocked", status: ["state": .string("blocked"), "kind": .string("auth")]).sign(unseen: false) == .key)
    #expect(try session(waiting: "blocked", status: ["state": .string("blocked")]).sign(unseen: false) == .hand)
    let failed = try session(waiting: "error", status: ["state": .string("error")])
    #expect(failed.sign(unseen: true) == .failed)
    #expect(failed.sign(unseen: false) == nil)
    let done = try session(waiting: "done", status: ["state": .string("done")])
    #expect(done.mark(rested: .awake) == .awake)
    #expect(done.sign(unseen: true) == .done)
    #expect(done.sign(unseen: false) == nil)
    #expect(done.alert == "Done")
    let elsewhere = try session(
        waiting: "blocked", status: ["state": .string("blocked"), "kind": .string("question")], kind: "remote", connection: "offline",
    )
    #expect(elsewhere.needsUser)
    #expect(elsewhere.sign(unseen: false) == .question)
    #expect(elsewhere.alert == nil)
}

@Test func `a terminal names where its program runs and whether as root`() throws {
    func session(context: [String: JSONValue]?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("local"), "name": .string("Amber Wren"),
            "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
            "connectionState": .string("local"), "sharedWith": .array([]),
        ]
        fields["context"] = context.map(JSONValue.object)
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
    }
    let remote = try session(context: [
        "place": .object(["kind": .string("remote"), "name": .string("build")]), "root": .bool(true),
    ])
    #expect(remote.place?.kind == .remote && remote.place?.name == "build")
    #expect(remote.runsAsRoot)
    #expect(remote.placeLabel == "On build")
    let container = try session(context: [
        "place": .object(["kind": .string("container"), "name": .string("debian")]), "root": .bool(false),
    ])
    #expect(container.placeLabel == "In the container debian")
    #expect(!container.runsAsRoot)
    let root = try session(context: ["root": .bool(true)])
    #expect(root.place == nil)
    #expect(root.runsAsRoot)
    let local = try session(context: nil)
    #expect(local.place == nil && !local.runsAsRoot && local.placeLabel == nil)
}
