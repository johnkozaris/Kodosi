import Foundation
@testable import KodosiDesktop
import Testing

@Test @MainActor func commandReplyMatchesRequestOperationAndAccount() async throws {
    var sent: [Data] = []
    let commands = CommandSink { data in sent.append(data); return 0 }
    commands.setAccount(userId: "owner", epoch: 4)
    var completed = false
    let request = Task { @MainActor in
        let reply = try await commands.request("session.close", ["requestId": .string("request")])
        completed = true
        return reply
    }
    await settle { sent.isEmpty }
    #expect(sent.count == 1)
    for event in try [
        runtimeEvent("session.result", epoch: 3, fields: ["requestId": .string("request"), "operation": .string("session.close")]),
        runtimeEvent("session.result", epoch: 4, userId: "someone-else", fields: ["requestId": .string("request"), "operation": .string("session.close")]),
        runtimeEvent("session.result", epoch: 4, fields: ["requestId": .string("request"), "operation": .string("session.rename")]),
        runtimeEvent("provider.reply", epoch: 4, fields: ["requestId": .string("request"), "operation": .string("session.close")]),
    ] {
        commands.receive(event)
    }
    await Task.yield()
    #expect(!completed)
    try commands.receive(runtimeEvent("session.result", epoch: 4, fields: ["requestId": .string("request"), "operation": .string("session.close")]))
    #expect(try await request.value.type == "session.result")
    #expect(completed)
}

@Test @MainActor func accountChangeCancelsPendingMutation() async throws {
    var sent = false
    let commands = CommandSink { _ in sent = true; return 0 }
    commands.setAccount(userId: "owner", epoch: 1)
    let request = Task { try await commands.request("session.rename") }
    await settle { !sent }
    commands.setAccount(userId: nil, epoch: 2)
    do {
        _ = try await request.value
        Issue.record("A pending action must not survive an account change")
    } catch {
        #expect(error as? RuntimeError == .accountChanged)
    }
}

@Test @MainActor func duplicateRequestCannotReplaceContinuation() async throws {
    var sent = false
    let commands = CommandSink { _ in sent = true; return 0 }
    let fields: [String: JSONValue] = ["requestId": .string("duplicate")]
    let first = Task { try await commands.request("mission.create", fields) }
    await settle { !sent }
    do {
        _ = try await commands.request("mission.create", fields)
        Issue.record("A duplicate request replaced its original continuation")
    } catch { #expect(error is RuntimeError) }
    commands.cancelPending()
    do {
        _ = try await first.value
        Issue.record("Expected cancellation")
    } catch {
        #expect(error as? RuntimeError == .unavailable)
    }
}

@Test @MainActor func synchronousRejectionDoesNotReportSuccess() async {
    let commands = CommandSink { _ in 5 }
    do {
        _ = try await commands.request("session.create")
        Issue.record("Rejected admission was treated as success")
    } catch { #expect(error as? RuntimeError == .rejected(5)) }
}

@MainActor private func settle(_ pending: () -> Bool) async {
    for _ in 0 ..< 100 where pending() {
        await Task.yield()
    }
}

@Test @MainActor func previewUsesNativeIdentityAndByteCursor() async throws {
    var requests: [[String: JSONValue]] = []
    let commands = CommandSink { data in
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
    try commands.receive(runtimeEvent("provider.reply", epoch: 0, userId: nil, fields: [
        "requestId": #require(discovery["requestId"]), "operation": .string("provider.discoverConversations"),
        "result": .object(["items": .array([conversation]), "hasMore": .bool(false), "responseBytes": .int(120)]),
    ]))
    await settle { browser.conversations.isEmpty }
    try browser.select(#require(browser.conversations.first))
    await settle { requests.count < 2 }
    let read = try #require(requests.last)
    #expect(read["type"] == .string("provider.readConversation"))
    #expect(read["nativeConversationId"] == .string("native-conversation"))
    #expect(read["workingDirectory"] == .string("/project"))
    try commands.receive(runtimeEvent("provider.reply", epoch: 0, userId: nil, fields: [
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
    try commands.receive(runtimeEvent("provider.reply", epoch: 0, userId: nil, fields: [
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
