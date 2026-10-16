import Foundation
@testable import Kodosi
import KodosiBridge
import KodosiKit
import Testing

@MainActor
struct AccountStoreTests {
    private let outbox = AccountOutbox()
    private let notices = NoticeCenter()
    private let store: AccountStore
    private let deletionPage = "https://auth.kodosi.com/realms/kodosi/protocol/openid-connect/auth?kc_action=delete_account"

    init() {
        store = AccountStore(commands: outbox.commands, browser: "app")
        store.notify = { [notices] message in notices.show(message) }
        store.reset(userId: "user-1")
    }

    @Test func devicesListPutsThisDeviceFirstAndTheRecoveryKeyLast() throws {
        try deliverDevices()
        #expect(store.orderedDevices.map(\.deviceId) == ["phone", "mac", "key"])
        #expect(store.hasRecoveryKey)
        #expect(store.devicesRevision == 1)
        let mac = try #require(store.devices.first { $0.deviceId == "mac" })
        let phone = try #require(store.devices.first { $0.deviceId == "phone" })
        #expect(store.canRemove(mac))
        #expect(!store.canRemove(phone))
    }

    @Test func removingADeviceSendsItsId() throws {
        try deliverDevices()
        let mac = try #require(store.devices.first { $0.deviceId == "mac" })
        store.revokeDevice(mac)
        #expect(outbox.sent.last?["type"] == .string("devices.revoke"))
        #expect(outbox.sent.last?["deviceId"] == .string("mac"))
    }

    @Test func approvingADeviceShowsWhichDeviceWasApproved() throws {
        store.approveDevice(code: "   ")
        #expect(outbox.sent.isEmpty)
        store.approveDevice(code: " abcd-efgh-jkmn \n")
        store.approveDevice(code: "WXYZ-WXYZ-WXYZ")
        #expect(outbox.types == ["devices.link.approve"])
        #expect(outbox.sent.last?["code"] == .string("abcd-efgh-jkmn"))
        #expect(store.approval == .approving)
        try store.receiveDevices(event("devices.link.resolved", ["deviceLabel": "MacBook Pro", "outcome": "approved"]))
        #expect(store.approval == .approved("MacBook Pro"))
        #expect(outbox.types.last == "devices.refresh")
        store.finishApproval()
        #expect(store.approval == nil)
    }

    @Test func failedApprovalStaysInTheSheetOnlyWhileApproving() throws {
        let failure = try event("devices.error", ["operation": "devices.link.approve", "message": "No device has this code."])
        #expect(!store.absorb(error: failure, message: "No device has this code."))
        store.approveDevice(code: "ABCD-EFGH-JKMN")
        #expect(store.absorb(error: failure, message: "No device has this code."))
        #expect(store.approval == .failed("No device has this code."))
    }

    @Test func recoveryKeyIsShownOnceItArrives() throws {
        store.createRecoveryKey()
        store.createRecoveryKey()
        #expect(outbox.types == ["devices.recovery.create"])
        #expect(store.creatingRecoveryKey)
        try store.receiveDevices(event("devices.recovery.created", ["key": "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH"]))
        #expect(store.newRecoveryKey == "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
        #expect(!store.creatingRecoveryKey)
        store.dismissRecoveryKey()
        #expect(store.newRecoveryKey == nil)
    }

    @Test func recoveryKeyFailureEndsTheWait() throws {
        store.createRecoveryKey()
        let failure = try event("devices.error", ["operation": "devices.recovery.create", "message": "Try again."])
        #expect(!store.absorb(error: failure, message: "Try again."))
        #expect(!store.creatingRecoveryKey)
    }

    @Test func accountDeletionOpensTheConfirmationPage() throws {
        store.deleteAccount()
        store.deleteAccount()
        #expect(outbox.types == ["auth.deleteAccount"])
        #expect(store.deletion == .requested)
        try store.receiveAuth(event("auth.deletion_pending", ["confirmationUri": deletionPage]))
        let page = try #require(URL(string: deletionPage))
        #expect(store.deletion == .confirming(page))
        #expect(store.deletionPage == page)
        store.closeDeletionPage()
        #expect(store.deletionPage == nil)
        store.reopenDeletionPage()
        #expect(store.deletionPage == page)
        store.cancelAccountDeletion()
        #expect(outbox.types.last == "auth.deleteAccount.cancel")
        #expect(store.deletion == nil)
        #expect(store.deletionPage == nil)
    }

    @Test func insecureDeletionPageIsNotOpened() throws {
        store.deleteAccount()
        try store.receiveAuth(event("auth.deletion_pending", ["confirmationUri": "http://auth.kodosi.com/delete"]))
        #expect(store.deletion == .confirming(nil))
        #expect(store.deletionPage == nil)
    }

    @Test func deletionFailureEndsTheRequestAndShowsTheMessage() throws {
        store.deleteAccount()
        let failure = try event("auth.error", ["operation": "auth.deleteAccount", "message": "Kodosi could not reach the server."])
        #expect(!store.absorb(error: failure, message: "Kodosi could not reach the server."))
        #expect(store.deletion == nil)
    }

    @Test func deletedAccountClearsTheDeletionAndSaysSo() throws {
        store.deleteAccount()
        try store.receiveAuth(event("auth.deletion_pending", ["confirmationUri": deletionPage]))
        try store.receiveAuth(event("auth.required", ["reason": "accountDeleted"]))
        #expect(store.deletion == nil)
        #expect(store.deletionPage == nil)
        #expect(notices.current?.message == "Your Kodosi account is deleted.")
    }

    @Test func authErrorWhileSignedInKeepsTheSignedInStage() throws {
        try store.receiveAuth(event("auth.ready", ["userId": "user-1", "enrolled": true]))
        #expect(store.isReady)
        let failure = try event("auth.error", ["operation": "auth.refresh", "message": "Offline."])
        #expect(!store.absorb(error: failure, message: "Offline."))
        #expect(store.stage == .signedIn)
    }

    @Test func authErrorDuringSignInFailsTheSignIn() throws {
        store.reset(userId: nil)
        try store.receiveAuth(event("auth.required", userId: nil, ["reason": "signedOut"]))
        store.signIn()
        #expect(outbox.types == ["auth.login.start"])
        let failure = try event("auth.error", userId: nil, ["operation": "auth.login.start", "message": "The sign-in service is down."])
        #expect(store.absorb(error: failure, message: "The sign-in service is down."))
        #expect(store.stage == .failed(message: "The sign-in service is down.", completed: 0))
    }

    @Test func signOutSendsLogout() {
        store.signOut()
        #expect(outbox.types == ["auth.logout"])
    }

    @Test func refreshDevicesWaitsForTheNextList() async throws {
        let refresh = Task { await store.refreshDevices() }
        for _ in 0 ..< 100 where outbox.sent.isEmpty {
            await Task.yield()
        }
        #expect(outbox.types == ["devices.refresh"])
        try deliverDevices()
        await refresh.value
        #expect(store.devicesRevision == 1)
    }

    @Test func resetForgetsDevicesAndPendingWork() throws {
        try deliverDevices()
        store.approveDevice(code: "ABCD-EFGH-JKMN")
        store.createRecoveryKey()
        store.deleteAccount()
        store.reset(userId: "user-2")
        #expect(store.devices.isEmpty)
        #expect(store.approval == nil)
        #expect(!store.creatingRecoveryKey)
        #expect(store.deletion == nil)
    }

    private func deliverDevices() throws {
        try store.receiveDevices(event("devices.list", [
            "selfDeviceId": "phone",
            "localDeviceEnrolled": true,
            "devices": [
                device("key", label: "Recovery key", recoveryKey: true),
                device("mac", label: "MacBook Pro"),
                device("phone", label: "iPhone"),
            ],
        ]))
    }

    private func device(_ id: String, label: String, recoveryKey: Bool = false) -> [String: Any] {
        ["deviceId": id, "label": label, "certIssuedAtMs": 1_790_000_000_000, "recoveryKey": recoveryKey]
    }
}

@MainActor
private final class AccountOutbox {
    let commands = CommandSink()
    private(set) var sent: [[String: JSONValue]] = []

    init() {
        commands.transport = { [weak self] data in
            RuntimeContract.verify(data)
            if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data) {
                self?.sent.append(command)
            }
            return Int32(KODOSI_FFI_OK)
        }
    }

    var types: [String] {
        sent.compactMap { $0["type"]?.stringValue }
    }
}

private func event(_ type: String, userId: String? = "user-1", _ fields: [String: Any] = [:]) throws -> RuntimeEvent {
    var payload = fields
    payload["type"] = type
    payload["accountUserId"] = userId ?? NSNull()
    payload["accountEpoch"] = 1
    return try JSONDecoder().decode(RuntimeEvent.self, from: JSONSerialization.data(withJSONObject: payload))
}
