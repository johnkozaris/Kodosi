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
                    directory: String = "/remote/project", restoredFrom: String? = nil,
                    ownerUserId: String? = nil, hostDeviceId: String? = nil) throws -> RuntimeSession
{
    var fields: [String: JSONValue] = [
        "id": .string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
        "kind": .string(kind), "name": .string("Terminal"), "workingDir": .string(directory),
        "isOwner": .bool(owner), "status": .string(status), "attention": .bool(false), "hidden": .bool(false), "connectionState": .string(connection),
        "sharedWith": .array([]),
    ]
    fields["restoredFrom"] = restoredFrom.map(JSONValue.string)
    fields["ownerUserId"] = ownerUserId.map(JSONValue.string)
    fields["hostDeviceId"] = hostDeviceId.map(JSONValue.string)
    return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
}

private func onMini(_ id: String, restoredFrom: String? = nil, owner: String = "ana") throws -> RuntimeSession {
    try runtimeSession(id, connection: "connected", restoredFrom: restoredFrom, ownerUserId: owner, hostDeviceId: "mini")
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

@Test @MainActor func `stage restoration is account scoped and survives delayed catalog`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageRestoration"))
    let session = try runtimeSession()
    let first = WorkbenchState(defaults: defaults)
    first.switchAccount("owner")
    first.showSession(session.id)
    first.sidebarCollapsed = true
    #expect(first.stagedSessionIds == [session.id])
    let restored = WorkbenchState(defaults: defaults)
    #expect(restored.sidebarCollapsed)
    restored.switchAccount("owner")
    #expect(restored.reconcile([]).isEmpty)
    let restarted = WorkbenchState(defaults: defaults)
    restarted.switchAccount("owner")
    #expect(restarted.reconcile([session]) == [session.id])
    #expect(restarted.stagedSessionIds == [session.id])
    restarted.switchAccount("another-user")
    #expect(restarted.stagedSessionIds.isEmpty)
    #expect(restarted.reconcile([session]).isEmpty)
    restarted.switchAccount("owner")
    #expect(restarted.reconcile([session]) == [session.id])
    restarted.dismissSession(session.id)
    #expect(restarted.reconcile([session]).isEmpty)
}

@Test @MainActor func `the signed out stage comes back after a restart`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageSignedOut"))
    let session = try runtimeSession()
    let first = WorkbenchState(defaults: defaults)
    first.switchAccount(nil)
    first.showSession(session.id)
    let restarted = WorkbenchState(defaults: defaults)
    restarted.switchAccount(nil)
    #expect(restarted.reconcile([session]) == [session.id])
    restarted.switchAccount("owner")
    #expect(restarted.reconcile([session]).isEmpty)
}

@Test @MainActor func `a terminal that the host started again takes the place of the one before`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageRestarted"))
    let before = try runtimeSession(kind: "local", connection: "local")
    let first = WorkbenchState(defaults: defaults)
    first.switchAccount(nil)
    first.showSession(before.id)
    let again = try runtimeSession("01992e43-53db-7040-8e02-f6ebf4149b30", kind: "local", connection: "local",
                                   restoredFrom: before.id)
    let restarted = WorkbenchState(defaults: defaults)
    restarted.switchAccount(nil)
    #expect(restarted.reconcile([again]) == [again.id])
    #expect(restarted.stagedSessionIds == [again.id])
}

@Test @MainActor func `restored terminals keep their order on the stage`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageRestartedOrder"))
    let ids = (0 ..< 4).map { "01992e43-53db-7040-8e02-f6ebf4149b4\($0)" }
    let first = WorkbenchState(defaults: defaults)
    first.switchAccount(nil)
    first.showSession(ids[0])
    first.showSession(ids[1])
    let earlier = try runtimeSession(ids[2], kind: "local", connection: "local", restoredFrom: ids[0])
    let later = try runtimeSession(ids[3], kind: "local", connection: "local", restoredFrom: ids[1])
    let restarted = WorkbenchState(defaults: defaults)
    restarted.switchAccount(nil)
    _ = restarted.reconcile([later])
    _ = restarted.reconcile([earlier, later])
    #expect(restarted.stagedSessionIds == [ids[2], ids[3]])
}

@Test @MainActor func `a terminal that another computer started again takes the place and focus of the one that ended`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollow"))
    let ids = (0 ..< 3).map { "01992e43-53db-7040-8e02-f6ebf4149b5\($0)" }
    let (ended, other) = try (onMini(ids[0]), onMini(ids[1]))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile([ended, other])
    workbench.showSession(ended.id)
    workbench.showSession(other.id)
    workbench.toggleFocus(ended.id)
    #expect(workbench.reconcile([other]).isEmpty)
    #expect(workbench.stagedSessionIds == [other.id])
    #expect(workbench.selectedSessionId == nil)
    #expect(workbench.focusedSessionId == nil)
    let again = try onMini(ids[2], restoredFrom: ended.id)
    #expect(workbench.reconcile([other, again]) == [again.id])
    #expect(workbench.stagedSessionIds == [again.id, other.id])
    #expect(workbench.selectedSessionId == again.id)
    #expect(workbench.focusedSessionId == again.id)
}

@Test @MainActor func `a restart takes no selection that the person gave to another terminal`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollowSelection"))
    let ids = (0 ..< 3).map { "01992e43-53db-7040-8e02-f6ebf4149b6\($0)" }
    let (ended, other) = try (onMini(ids[0]), onMini(ids[1]))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile([ended, other])
    workbench.showSession(ended.id)
    workbench.showSession(other.id)
    workbench.toggleFocus(ended.id)
    _ = workbench.reconcile([other])
    workbench.selectSession(other.id)
    _ = try workbench.reconcile([other, onMini(ids[2], restoredFrom: ended.id)])
    #expect(workbench.stagedSessionIds == [ids[2], other.id])
    #expect(workbench.selectedSessionId == other.id)
    #expect(workbench.focusedSessionId == nil)
}

@Test @MainActor func `terminals that ended together come back in their places and only the selected one takes the selection`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollowTogether"))
    let ids = (0 ..< 4).map { "01992e43-53db-7040-8e02-f6ebf4149b9\($0)" }
    let (first, second) = try (onMini(ids[0]), onMini(ids[1]))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile([first, second])
    workbench.showSession(first.id)
    workbench.showSession(second.id)
    _ = workbench.reconcile([])
    let restarts = try [onMini(ids[3], restoredFrom: second.id), onMini(ids[2], restoredFrom: first.id)]
    #expect(Set(workbench.reconcile(restarts)) == [ids[2], ids[3]])
    #expect(workbench.stagedSessionIds == [ids[2], ids[3]])
    #expect(workbench.selectedSessionId == ids[3])
}

@Test @MainActor func `a terminal that is still listed moves to its restart at once`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollowListed"))
    let ids = (0 ..< 2).map { "01992e43-53db-7040-8e02-f6ebf4149b7\($0)" }
    let away = try onMini(ids[0])
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile([away])
    workbench.showSession(away.id)
    let again = try onMini(ids[1], restoredFrom: away.id)
    #expect(workbench.reconcile([away, again]) == [again.id])
    #expect(workbench.stagedSessionIds == [again.id])
    #expect(workbench.selectedSessionId == again.id)
}

@Test @MainActor func `a terminal follows no restart on another computer, of another person or after a close`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollowRefused"))
    let ids = (0 ..< 6).map { "01992e43-53db-7040-8e02-f6ebf4149b8\($0)" }
    let (ended, closed) = try (onMini(ids[0]), onMini(ids[1]))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile([ended, closed])
    workbench.showSession(ended.id)
    workbench.showSession(closed.id)
    workbench.noteClosing(closed.id)
    _ = workbench.reconcile([ended])
    #expect(workbench.selectedSessionId == ended.id)
    _ = workbench.reconcile([])
    let elsewhere = try runtimeSession(ids[2], connection: "connected", restoredFrom: ended.id, ownerUserId: "ana", hostDeviceId: "air")
    let someone = try onMini(ids[3], restoredFrom: ended.id, owner: "bob")
    let reopened = try onMini(ids[4], restoredFrom: closed.id)
    #expect(workbench.reconcile([elsewhere, someone, reopened]).isEmpty)
    #expect(workbench.stagedSessionIds.isEmpty)
    let again = try onMini(ids[5], restoredFrom: ended.id)
    #expect(workbench.reconcile([elsewhere, someone, reopened, again]) == [again.id])
}

@Test @MainActor func `a terminal whose close failed still follows its restart`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollowFailedClose"))
    let ids = (0 ..< 2).map { "01992e43-53db-7040-8e02-f6ebf4149ba\($0)" }
    let ended = try onMini(ids[0])
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile([ended])
    workbench.showSession(ended.id)
    workbench.noteClosing(ended.id)
    workbench.cancelClosing(ended.id)
    _ = workbench.reconcile([])
    let again = try onMini(ids[1], restoredFrom: ended.id)
    #expect(workbench.reconcile([again]) == [again.id])
    #expect(workbench.stagedSessionIds == [again.id])
}

@Test @MainActor func `stage restoration opens sessions whose host is offline so they connect when it is back`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageRestorationOffline"))
    let session = try runtimeSession(status: "offline")
    #expect(session.statusLabel == "Remote computer is offline")
    #expect(try runtimeSession().statusLabel == "Not connected")
    let first = WorkbenchState(defaults: defaults)
    first.switchAccount("owner")
    first.showSession(session.id)
    let restarted = WorkbenchState(defaults: defaults)
    restarted.switchAccount("owner")
    #expect(restarted.reconcile([session]) == [session.id])
    #expect(restarted.stagedSessionIds == [session.id])
}

@Test @MainActor func `stage limit keeps existing terminals and allows reopening`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageLimit"))
    let workbench = WorkbenchState(defaults: defaults)
    let ids = (0 ..< 7).map { _ in UUIDv7.generate() }
    for id in ids {
        workbench.showSession(id)
    }
    #expect(workbench.stagedSessionIds == Array(ids.prefix(6)))
    workbench.showSession(ids[0])
    #expect(workbench.selectedSessionId == ids[0])
    workbench.dismissSession(ids[1])
    workbench.showSession(ids[6])
    #expect(workbench.stagedSessionIds.count == 6)
    #expect(workbench.selectedSessionId == ids[6])
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
    let groups = SessionFolderGroup.groups([first, second, remote])
    #expect(groups.count == 2)
    #expect(groups[0].sessions.map(\.id) == [first.id, second.id])
    #expect(groups[1].host != nil)
    let defaults = try #require(EphemeralUserDefaults(prefix: "FolderNavigation"))
    let workbench = WorkbenchState(defaults: defaults)
    workbench.showSession(first.id)
    workbench.showSession(second.id)
    workbench.selectAdjacentSession(offset: 1)
    #expect(workbench.selectedSessionId == first.id)
    workbench.sidebarCollapsed = true
    #expect(workbench.stagedSessionIds == [first.id, second.id])
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
    #expect(SessionFolderGroup.groups([windows]).map(\.name) == ["project"])
}

@Test func `provider title does not replace the sidebar name`() throws {
    let session = try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode([
        "id": JSONValue.string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("Green Gecko"), "title": .string("GitHub Copilot"),
        "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
        "connectionState": .string("local"), "sharedWith": .array([]),
    ]))
    #expect(session.name == "Green Gecko")
    #expect(session.activity == "GitHub Copilot")
    #expect(!session.isWorking)
}

@Test func `terminal title spinner marks an agent at work`() throws {
    func session(title: String?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("local"), "name": .string("Amber Wren"), "program": .string("claude"),
            "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
            "connectionState": .string("local"), "sharedWith": .array([]),
        ]
        if let title {
            fields["title"] = .string(title)
        }
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
    }
    let working = try session(title: "\u{2810} Draft the release notes")
    #expect(working.isWorking)
    #expect(working.activity == "Draft the release notes")
    #expect(working.agent == .claude)
    let resting = try session(title: "\u{2733} Draft the release notes")
    #expect(!resting.isWorking)
    #expect(resting.activity == "Draft the release notes")
    #expect(try session(title: "Amber Wren").activity == nil)
    #expect(try session(title: nil).activity == nil)
    #expect(try !session(title: nil).isWorking)
}

@Test func `a program status report is the state of its terminal`() throws {
    func session(title: String? = nil, status: [String: JSONValue]) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("local"), "name": .string("Amber Wren"), "program": .string("claude"),
            "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
            "connectionState": .string("local"), "sharedWith": .array([]),
            "programStatus": .object(status),
        ]
        if let title {
            fields["title"] = .string(title)
        }
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
    }
    let working = try session(title: "Draft the release notes", status: ["state": .string("working"), "progress": .int(40)])
    #expect(working.isWorking)
    #expect(!working.needsUser)
    #expect(working.waitState == nil)
    #expect(working.activity == "Draft the release notes")
    #expect(working.progress == 40)
    #expect(working.mark(rested: .asleep) == .progress(40))
    #expect(working.sign(unseen: false) == nil)
    #expect(try session(status: ["state": .string("working")]).mark(rested: .asleep) == .working)
    let idle = try session(title: "\u{2810} Draft the release notes", status: ["state": .string("idle"), "app": .string("claude-code")])
    #expect(!idle.isWorking)
    #expect(idle.mark(rested: .asleep) == .asleep)
    #expect(idle.sign(unseen: false) == nil)
    #expect(idle.sign(unseen: true) == .changed)
    #expect(idle.activity == "Draft the release notes")
    let approval = try session(status: ["state": .string("blocked"), "kind": .string("permission"), "message": .string("Allow the command?")])
    #expect(approval.needsUser)
    #expect(approval.waitState == .blocked)
    #expect(!approval.isWorking)
    #expect(approval.activity == "Allow the command?")
    #expect(approval.mark(rested: .asleep) == .asleep)
    #expect(approval.sign(unseen: false) == .hand)
    let task = try session(status: ["state": .string("blocked"), "kind": .string("question"), "title": .string("Review the plan")])
    #expect(task.activity == "Review the plan")
    #expect(task.sign(unseen: true) == .question)
    let signIn = try session(status: ["state": .string("blocked"), "kind": .string("auth")])
    #expect(signIn.activity == nil)
    #expect(signIn.sign(unseen: false) == .key)
    #expect(try session(status: ["state": .string("blocked")]).sign(unseen: false) == .hand)
    let failed = try session(title: "Deploy", status: ["state": .string("error")])
    #expect(failed.waitState == .error)
    #expect(failed.activity == "Deploy")
    #expect(failed.sign(unseen: true) == .failed)
    #expect(failed.sign(unseen: false) == nil)
    let done = try session(title: "Deploy", status: ["state": .string("done")])
    #expect(done.waitState == .done)
    #expect(done.activity == "Deploy")
    #expect(done.mark(rested: .awake) == .awake)
    #expect(done.sign(unseen: true) == .done)
    #expect(done.sign(unseen: false) == nil)
}

@Test func `a room terminal that is not open here shows the status of its host`() throws {
    func session(title: String? = nil, status: [String: JSONValue]?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("remote"), "name": .string("Amber Wren"), "program": .string("claude"),
            "isOwner": .bool(false), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
            "connectionState": .string("offline"), "sharedWith": .array([]),
        ]
        fields["title"] = title.map(JSONValue.string)
        fields["programStatus"] = status.map(JSONValue.object)
        return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
    }
    let asking = try session(status: ["state": .string("blocked"), "kind": .string("question")])
    #expect(asking.needsUser)
    #expect(asking.mark(rested: .asleep) == .asleep)
    #expect(asking.sign(unseen: false) == .question)
    #expect(asking.alert == nil)
    #expect(try session(status: ["state": .string("working")]).isWorking)
    #expect(try !session(title: "\u{2810} Draft the release notes", status: nil).isWorking)
}

@Test func `a terminal names where its program runs and whether as root`() throws {
    func session(context: [String: JSONValue]?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("local"), "name": .string("Amber Wren"),
            "isOwner": .bool(true), "status": .string("running"), "attention": .bool(false), "hidden": .bool(false),
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
