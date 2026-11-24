import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

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

private func settle(_ pending: () -> Bool) async {
    for _ in 0 ..< 100 where pending() {
        await Task.yield()
    }
}
