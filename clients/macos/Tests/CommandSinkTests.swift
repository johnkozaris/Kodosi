import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@Test func `command reply matches request and operation`() async throws {
    var sent: [Data] = []
    let commands = CommandSink()
    commands.transport { data in sent.append(data); return 0 }
    commands.setAccount(userId: "owner", epoch: 4)
    var completed = false
    let request = Task {
        let reply = try await commands.request("session.close", ["requestId": .string("request")])
        completed = true
        return reply
    }
    await settle { sent.isEmpty }
    #expect(sent.count == 1)
    for event in try [
        runtimeEvent("session.result", epoch: 4, fields: ["requestId": .string("other"), "operation": .string("session.close")]),
        runtimeEvent("session.result", epoch: 4, fields: ["requestId": .string("request"), "operation": .string("session.rename")]),
        runtimeEvent("provider.result", epoch: 4, fields: ["requestId": .string("request"), "operation": .string("session.close")]),
    ] {
        commands.receive(event)
    }
    await Task.yield()
    #expect(!completed)
    try commands.receive(runtimeEvent("session.result", epoch: 4, fields: ["requestId": .string("request"), "operation": .string("session.close")]))
    #expect(try await request.value.type == "session.result")
    #expect(completed)
}

@Test func `account change cancels pending mutation`() async {
    var sent = false
    let commands = CommandSink()
    commands.transport { _ in sent = true; return 0 }
    commands.setAccount(userId: "owner", epoch: 1)
    let request = Task { try await commands.request("session.rename") }
    await settle { !sent }
    commands.setAccount(userId: nil, epoch: 2)
    await #expect(throws: RuntimeError.accountChanged) { try await request.value }
}

@Test func `duplicate request cannot replace continuation`() async {
    var sent = false
    let commands = CommandSink()
    commands.transport { _ in sent = true; return 0 }
    let fields: [String: JSONValue] = ["requestId": .string("duplicate")]
    let first = Task { try await commands.request("project.create", fields) }
    await settle { !sent }
    await #expect(throws: RuntimeError.self) { try await commands.request("project.create", fields) }
    commands.cancelPending()
    await #expect(throws: RuntimeError.unavailable) { try await first.value }
}

@Test func `synchronous rejection does not report success`() async {
    let commands = CommandSink { _ in 5 }
    await #expect(throws: RuntimeError.rejected(5)) { try await commands.request("session.create") }
}

@Test func `an error shows once and only for a request of the app`() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "CommandErrors"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.required", fields: ["reason": .string("signedOut")]))
    let creating = Task { try await app.commandSink.request("project.create", ["requestId": .string("create"), "name": .string("Launch")]) }
    await settle { !sent.contains { $0["type"] == .string("project.create") } }
    try app.receive(runtimeEvent("project.error", fields: [
        "requestId": .string("create"), "operation": .string("project.create"), "message": .string("Use another name."),
    ]))
    let error = await #expect(throws: (any Error).self) { try await creating.value }
    #expect(error?.localizedDescription == "Use another name.")
    #expect(app.errorMessage == nil)

    try app.receive(runtimeEvent("friends.error", fields: ["operation": .string("friends.remove"), "message": .string("Try again.")]))
    #expect(app.errorMessage == "Try again.")
}

@Test func `a late answer to a request that its caller left shows nowhere`() async throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "LeftRequest"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner")]))
    let listing = Task { try await app.commandSink.request("folders.list", ["requestId": .string("folders")]) }
    await settle { !sent.contains { $0["type"] == .string("folders.list") } }
    listing.cancel()
    await #expect(throws: CancellationError.self) { try await listing.value }
    try app.receive(runtimeEvent("folders.error", fields: [
        "requestId": .string("folders"), "operation": .string("folders.list"), "message": .string("Studio is away."),
    ]))
    #expect(app.errorMessage == nil)
}

@Test func `a failed friend request of this app shows its error`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "FriendRequestError"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    app.perform("friends.request.send", ["username": .string("nobody"), "requestId": .string(UUIDv7.generate())])
    let request = try #require(sent.last { $0["type"] == .string("friends.request.send") }?["requestId"])
    try app.receive(runtimeEvent("friends.error", fields: [
        "requestId": request, "operation": .string("friends.request.send"), "message": .string("No such person."),
    ]))
    #expect(app.errorMessage == "No such person.")
}

@Test func `a failed code for a new device shows only in its sheet`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "DeviceOfferError"))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    defer { app.shutdownProcess() }
    var sent: [[String: JSONValue]] = []
    app.commandSink.transport { data in
        if let fields = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            sent.append(fields)
        }
        return 0
    }
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    app.account.offerDevice()
    let request = try #require(sent.last { $0["type"] == .string("devices.link.offer") }?["requestId"])
    try app.receive(runtimeEvent("devices.error", fields: [
        "requestId": request, "operation": .string("devices.link.offer"), "message": .string("The server is away."),
    ]))
    #expect(app.account.offer == .failed("The server is away."))
    #expect(app.errorMessage == nil)
}

private func settle(_ pending: () -> Bool) async {
    for _ in 0 ..< 100 where pending() {
        await Task.yield()
    }
}

@Test func `preview uses native identity and byte cursor`() async throws {
    var requests: [[String: JSONValue]] = []
    let commands = CommandSink()
    commands.transport { data in
        if let value = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
            requests.append(value)
        }
        return 0
    }
    let browser = ConversationBrowser(commands: commands, directory: "/project")
    browser.load()
    await settle { requests.isEmpty }
    let discovery = try #require(requests.first)
    #expect(discovery["type"] == .string("provider.discoverConversations"))
    let conversation: JSONValue = .object([
        "provider": .string("claude"), "nativeConversationId": .string("native-conversation"),
        "workingDirectory": .string("/project"), "title": .string("Saved work"),
    ])
    try commands.receive(runtimeEvent("provider.result", epoch: 0, userId: nil, fields: [
        "requestId": #require(discovery["requestId"]), "operation": .string("provider.discoverConversations"),
        "result": .object(["items": .array([conversation]), "hasMore": .bool(false)]),
    ]))
    await settle { browser.conversations.isEmpty }
    try browser.select(#require(browser.conversations.first))
    await settle { requests.count < 2 }
    let read = try #require(requests.last)
    #expect(read["type"] == .string("provider.readConversation"))
    #expect(read["nativeConversationId"] == .string("native-conversation"))
    #expect(read["workingDirectory"] == .string("/project"))
    try commands.receive(runtimeEvent("provider.result", epoch: 0, userId: nil, fields: [
        "requestId": #require(read["requestId"]), "operation": .string("provider.readConversation"),
        "result": .object([
            "entries": .array([.object(["role": .string("user"), "content": .string("Earlier task")])]),
            "nextBeforeByte": .int(256), "sourceFileBytes": .int(1024), "readBytes": .int(768), "sourceRecords": .int(1),
        ]),
    ]))
    await settle { browser.nextBeforeByte == nil }
    #expect(browser.entries.first?.content == "Earlier task")
    browser.read(older: true)
    await settle { requests.count < 3 }
    #expect(requests.last?["beforeByte"] == .int(256))
    #expect(requests.last?["beforeLine"] == nil)
    #expect(!requests.contains { $0["type"] == .string("session.create") })
    let earlier = try #require(requests.last)
    try commands.receive(runtimeEvent("provider.result", epoch: 0, userId: nil, fields: [
        "requestId": #require(earlier["requestId"]), "operation": .string("provider.readConversation"),
        "result": .object([
            "entries": .array([.object(["role": .string("assistant"), "content": .string("Older page")])]),
            "sourceFileBytes": .int(1024), "readBytes": .int(256), "sourceRecords": .int(1),
        ]),
    ]))
    await settle { browser.reading }
    #expect(browser.entries.count == 1)
    #expect(browser.entries.first?.content == "Older page")
    #expect(browser.canReadNewer)
    browser.latest()
    await settle { requests.count < 4 }
    #expect(requests.last?["beforeByte"] == nil)
    let latest = try #require(requests.last)
    try commands.receive(runtimeEvent("provider.error", epoch: 0, userId: nil, fields: [
        "requestId": #require(latest["requestId"]), "operation": .string("provider.readConversation"),
        "message": .string("Read failed"),
    ]))
    await settle { browser.reading }
    #expect(browser.pageNumber == 2)
    #expect(browser.entries.first?.content == "Older page")
    browser.newer()
    await settle { requests.count < 5 }
    #expect(requests.last?["beforeByte"] == nil)
    browser.cancel()
    commands.cancelPending()
}
