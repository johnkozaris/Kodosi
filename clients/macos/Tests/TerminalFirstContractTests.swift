import Foundation
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
                    connection: String = "offline", status: String = "running", owner: Bool = true) throws -> RuntimeSession
{
    try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode([
        "id": JSONValue.string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
        "kind": .string(kind), "name": .string("Terminal"), "workingDir": .string("/remote/project"),
        "isOwner": .bool(owner), "status": .string(status), "connectionState": .string(connection),
        "sharedWith": .array([]),
    ]))
}

@Test func eventEnvelopeRequiresExplicitAccountAndExactEpoch() throws {
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

@Test func envelopeCannotBeOverriddenByCommandFields() throws {
    let command = RuntimeCommand(type: "session.list", fields: ["accountEpoch": .int(99), "accountUserId": .string("wrong")])
    let data = try JSONEncoder().encode(RuntimeCommandEnvelope(accountUserId: nil, accountEpoch: UInt64.max, command: command))
    let decoded = try JSONDecoder().decode(RuntimeEvent.self, from: data)
    #expect(decoded.accountEpoch == UInt64.max)
    #expect(decoded.accountUserId == nil)
    #expect(decoded.type == "session.list")
}

@Test func remoteSessionNeverTreatsHostDirectoryAsLocalPath() throws {
    let remote = try runtimeSession(connection: "connected", owner: false)
    #expect(remote.localDirectory == nil)
    #expect(remote.canControl)
    #expect(remote.canOpen)
    let local = try runtimeSession(kind: "local", connection: "local")
    #expect(local.localDirectory == "/remote/project")
    #expect(local.canControl)
    #expect(try !runtimeSession(connection: "blocked").canControl)
}

@Test func deepLinksOnlyIdentifyRealSessionUUIDs() throws {
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

@Test @MainActor func stageRestorationIsAccountScopedAndSurvivesDelayedCatalog() throws {
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

@Test @MainActor func stageRestorationStagesButDoesNotOpenSessionsWhoseHostIsOffline() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageRestorationOffline"))
    let session = try runtimeSession(status: "reconnecting")
    #expect(session.statusLabel == "Its computer is offline")
    #expect(try runtimeSession().statusLabel == "Not connected")
    let first = WorkbenchState(defaults: defaults)
    first.switchAccount("owner")
    first.showSession(session.id)
    let restarted = WorkbenchState(defaults: defaults)
    restarted.switchAccount("owner")
    #expect(restarted.reconcile([session]).isEmpty)
    #expect(restarted.stagedSessionIds == [session.id])
}

@Test @MainActor func stageLimitKeepsExistingTerminalsAndAllowsReopening() throws {
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

@Test func eventInboxIsBoundedOrderedAndClosesWaiter() async {
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

@Test func terminalControlInboxEnforcesItsOwnCountAndByteBudget() async {
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

@Test @MainActor func folderGroupsAndNavigationFollowTheWorkingSession() throws {
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

@Test func providerTitleDoesNotReplaceTheSidebarName() throws {
    let session = try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode([
        "id": JSONValue.string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
        "kind": .string("local"), "name": .string("Green Gecko"), "title": .string("GitHub Copilot"),
        "isOwner": .bool(true), "status": .string("running"), "connectionState": .string("local"), "sharedWith": .array([]),
    ]))
    #expect(session.name == "Green Gecko")
    #expect(session.activity == "GitHub Copilot")
    #expect(!session.isWorking)
}

@Test func terminalTitleSpinnerMarksAnAgentAtWork() throws {
    func session(title: String?) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("local"), "name": .string("Amber Wren"), "program": .string("claude"),
            "isOwner": .bool(true), "status": .string("running"), "connectionState": .string("local"), "sharedWith": .array([]),
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

@Test func aProgramStatusReportIsTheStateOfItsTerminal() throws {
    func session(title: String? = nil, status: [String: JSONValue]) throws -> RuntimeSession {
        var fields: [String: JSONValue] = [
            "id": .string(UUIDv7.generate()), "incarnationId": .string(UUIDv7.generate()),
            "kind": .string("local"), "name": .string("Amber Wren"), "program": .string("claude"),
            "isOwner": .bool(true), "status": .string("running"), "connectionState": .string("local"), "sharedWith": .array([]),
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
    #expect(approval.mark(rested: .asleep) == .asks)
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
