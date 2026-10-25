import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

private func onMini(_ id: String, restoredFrom: String? = nil, owner: String = "ana") throws -> RuntimeSession {
    try runtimeSession(id, connection: "connected", restoredFrom: restoredFrom, ownerUserId: owner, hostDeviceId: "mini")
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

@Test @MainActor func `terminals that ended one after another come back in their places`() throws {
    let defaults = try #require(EphemeralUserDefaults(prefix: "StageFollowInTurn"))
    let ids = (0 ..< 6).map { "01992e43-53db-7040-8e02-f6ebf4149bb\($0)" }
    let ended = try ids[0 ..< 3].map { try onMini($0) }
    let workbench = WorkbenchState(defaults: defaults)
    workbench.switchAccount("ana")
    _ = workbench.reconcile(ended)
    for session in ended {
        workbench.showSession(session.id)
    }
    _ = workbench.reconcile([ended[1], ended[2]])
    _ = workbench.reconcile([ended[1]])
    _ = workbench.reconcile([])
    let restarts = try (0 ..< 3).reversed().map { try onMini(ids[$0 + 3], restoredFrom: ended[$0].id) }
    _ = workbench.reconcile(restarts)
    #expect(workbench.stagedSessionIds == [ids[3], ids[4], ids[5]])
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
