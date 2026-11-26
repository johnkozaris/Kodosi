import Foundation
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct SharingApprovalsTests {
    private let commands = CommandSink()
    private let approvals: SharingApprovals
    private let log = ProjectCommandLog()
    private let first = UUID().uuidString.lowercased()
    private let second = UUID().uuidString.lowercased()

    init() {
        let log = log
        commands.transport = { data in
            RuntimeContract.verify(data)
            log.append(data)
            return Int32(KODOSI_FFI_OK)
        }
        commands.setAccount(userId: "user-1", epoch: 1)
        approvals = SharingApprovals(commands: commands)
    }

    @Test func `a question that comes again is asked once, in the place where it first came`() throws {
        try ask(first)
        try ask(second)
        try ask(first)
        #expect(approvals.waiting.map(\.id) == [first, second])
        #expect(approvals.current?.title == "Share “build” with Ana?")
        #expect(approvals.current?.from == .command)
    }

    @Test func `a question that ends is no longer asked, and the next one is`() throws {
        try ask(first)
        try ask(second)
        try deliver("sharing.approvalEnded", ["approvalId": first, "outcome": "expired"])
        #expect(approvals.current?.id == second)
        try deliver("sharing.approvalEnded", ["approvalId": second, "outcome": "allowed"])
        #expect(approvals.current == nil)
    }

    @Test func `the answer goes to the runtime once, and a copy that arrives meanwhile is not asked`() async throws {
        try ask(first)
        try ask(second)
        let question = try #require(approvals.current)
        approvals.answer(question, allow: true)
        approvals.answer(question, allow: false)
        #expect(approvals.current?.id == second)
        let answer = try await log.next("sharing.answer")
        #expect(answer["approvalId"] as? String == first)
        #expect(answer["allow"] as? Bool == true)
        try ask(first)
        #expect(approvals.waiting.map(\.id) == [second])
        try deliver("sharing.result", ["requestId": answer["requestId"] ?? "", "operation": "sharing.answer"])
        #expect(log.take("sharing.answer") == nil)
    }

    @Test func `an answer that comes too late shows the reason`() async throws {
        var shown: [String] = []
        approvals.notify = { shown.append($0) }
        try ask(first)
        try approvals.answer(#require(approvals.current), allow: false)
        let answer = try await log.next("sharing.answer")
        try deliver("sharing.error", [
            "requestId": answer["requestId"] ?? "", "operation": "sharing.answer", "message": "This request has already ended.",
        ])
        for _ in 0 ..< 100 where shown.isEmpty {
            await Task.yield()
        }
        #expect(shown == ["This request has already ended."])
    }

    @Test func `a request that the owner refused ends as refused, not as a failure to show`() async throws {
        let projectId = UUID().uuidString.lowercased()
        let inviting = Task { try await commands.request("project.invite", ["projectId": .string(projectId), "userId": .string("user-2")]) }
        let invite = try await log.next("project.invite")
        try deliver("project.error", [
            "requestId": invite["requestId"] ?? "", "operation": "project.invite", "projectId": projectId,
            "message": "You did not allow this in Kodosi.", "refused": true,
        ])
        await #expect(throws: RuntimeError.refused("You did not allow this in Kodosi.")) { try await inviting.value }
    }

    private func ask(_ id: String) throws {
        try deliver("sharing.approval", [
            "approvalId": id, "operation": "session.share", "title": "Share “build” with Ana?",
            "message": "Ana gets full control: they can type anything as you on Studio. The kodosi command asked for this.",
            "confirm": "Share", "from": "command",
        ])
    }

    private func deliver(_ type: String, _ fields: [String: Any]) throws {
        var payload = fields
        payload["type"] = type
        payload["accountUserId"] = "user-1"
        payload["accountEpoch"] = 1
        let data = try JSONSerialization.data(withJSONObject: payload)
        let event = try JSONDecoder().decode(RuntimeEvent.self, from: data)
        commands.receive(event)
        try approvals.receive(event)
    }
}
