import Foundation
import KodosiBridge
import KodosiDesign
@testable import KodosiDesktop
import Testing

extension CommandSink {
    /// For a test that sends from the main actor alone.
    func transport(_ answer: @escaping @MainActor (Data) -> Int32?) {
        transport = { data in MainActor.assumeIsolated { answer(data) } }
    }
}

func runtimeEvent(_ type: String, epoch: UInt64 = 1, userId: String? = "owner", fields: [String: JSONValue] = [:]) throws -> RuntimeEvent {
    var value = fields
    value["type"] = .string(type)
    value["accountEpoch"] = .uint(epoch)
    value["accountUserId"] = userId.map(JSONValue.string) ?? .null
    return try JSONDecoder().decode(RuntimeEvent.self, from: JSONEncoder().encode(value))
}

func runtimeReady(epoch: UInt64 = 1, userId: String? = "owner") throws -> RuntimeEvent {
    try runtimeEvent("system.ready", epoch: epoch, userId: userId, fields: [
        "protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion)), "awaySeconds": .uint(180),
    ])
}

func runtimeSession(_ id: String = "01992e43-53db-7040-8e02-f6ebf4149b28", kind: String = "remote",
                    connection: String = "offline", status: String = "running", owner: Bool = true,
                    restoredFrom: String? = nil, ownerUserId: String? = nil, hostDeviceId: String? = nil) throws -> RuntimeSession
{
    var fields: [String: JSONValue] = [
        "id": .string(id), "incarnationId": .string("01992e43-53db-7040-8e02-f6ebf4149b29"),
        "kind": .string(kind), "name": .string("Terminal"), "workingDir": .string("/remote/project"),
        "isOwner": .bool(owner), "status": .string(status), "attention": .bool(false), "hidden": .bool(false), "working": .bool(false),
        "connectionState": .string(connection),
        "sharedWith": .array([]),
    ]
    fields["restoredFrom"] = restoredFrom.map(JSONValue.string)
    fields["ownerUserId"] = ownerUserId.map(JSONValue.string)
    fields["hostDeviceId"] = hostDeviceId.map(JSONValue.string)
    return try JSONDecoder().decode(RuntimeSession.self, from: JSONEncoder().encode(fields))
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
    #expect(try runtimeSession(connection: "connected", status: "offline").isTroubled)
}

@Test func `event inbox keeps its order and closes its waiter`() async {
    let inbox = RuntimeEventInbox()
    for index in 0 ..< 200 {
        #expect(inbox.push(Data("\(index)".utf8)))
    }
    for index in 0 ..< 100 {
        #expect(await inbox.next() == .data(Data("\(index)".utf8)))
    }
    for index in 200 ..< 256 {
        #expect(inbox.push(Data("\(index)".utf8)))
    }
    for index in 100 ..< 256 {
        #expect(await inbox.next() == .data(Data("\(index)".utf8)))
    }
    let pending = Task { await inbox.next() }
    inbox.close()
    #expect(await pending.value == nil)
    #expect(!inbox.push(Data()))
    inbox.reopen()
    #expect(inbox.push(Data("restarted".utf8)))
    #expect(await inbox.next() == .data(Data("restarted".utf8)))
}

@Test func `a full inbox drops all that waits and says so before the newer data`() async {
    let inbox = RuntimeEventInbox(maximumBytes: 8, maximumCount: 2)
    #expect(inbox.push(Data([1, 2, 3, 4])))
    #expect(inbox.push(Data([5, 6, 7, 8])))
    #expect(!inbox.push(Data([9])))
    #expect(inbox.push(Data([10])))
    guard case let .behind(first)? = await inbox.next() else { Issue.record("no drop"); return }
    #expect(first.events == 3 && first.bytes == 9)
    #expect(await inbox.next() == .data(Data([10])))
    #expect(!inbox.push(Data(repeating: 0, count: 9)))
    guard case let .behind(second)? = await inbox.next() else { Issue.record("no drop"); return }
    #expect(second.events == 1 && second.bytes == 9)
    inbox.close()
}
