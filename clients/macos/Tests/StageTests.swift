import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

private func onMini(_ id: String, restoredFrom: String? = nil, owner: String = "ana") throws -> RuntimeSession {
    try runtimeSession(id, connection: "connected", restoredFrom: restoredFrom, ownerUserId: owner, hostDeviceId: "mini")
}

@Test func `stage restoration is account scoped and survives delayed catalog`() throws {
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

@Test func `a terminal that the host started again takes the place of the one before`() throws {
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

@Test func `a terminal that another computer started again takes the place and focus of the one that ended`() throws {
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

@Test func `a terminal follows no restart on another computer, of another person or after a close`() throws {
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
