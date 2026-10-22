import Foundation
import KodosiBridge
@testable import KodosiDesktop
import Testing

@MainActor
private func signedOutApp(prefix: String) throws -> (AppDependencies, SignInRecord) {
    let defaults = try #require(EphemeralUserDefaults(prefix: prefix))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    let sent = Box<[String]>([])
    let opened = Box<[URL]>([])
    let raised = Box<Int>(0)
    app.commandSink.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data), let type = command["type"]?.stringValue {
            sent.value.append([type, command["browser"]?.stringValue].compactMap(\.self).joined(separator: " "))
        }
        return 0
    }
    app.openExternalURL = { opened.value.append($0) }
    app.bringToFront = { raised.value += 1 }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(Int64(RuntimeHandle.expectedProtocolVersion))]))
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("signedOut")]))
    return (app, SignInRecord(sentCommands: sent, openedPages: opened, raises: raised))
}

private struct SignInRecord {
    let sentCommands: Box<[String]>
    let openedPages: Box<[URL]>
    let raises: Box<Int>
    var sent: [String] {
        sentCommands.value
    }

    var opened: [URL] {
        openedPages.value
    }
}

private final class Box<T>: @unchecked Sendable {
    var value: T
    init(_ value: T) {
        self.value = value
    }
}

private let page = "https://auth.example/realms/kodosi/protocol/openid-connect/auth?client_id=kodosi-app&state=s"
private let browserPage: [String: JSONValue] = ["url": .string(page)]

@Test @MainActor func `sign in walks through the steps and only opens the browser once`() throws {
    let (app, record) = try signedOutApp(prefix: "SignInSteps")
    #expect(app.signInPrompt == "Sign in")
    app.beginSignIn()
    #expect(app.signInPresented)
    #expect(app.account.stage == .starting)
    #expect(record.sent.last == "auth.login.start system")
    try app.receive(runtimeEvent("auth.browser", epoch: 0, userId: nil, fields: browserPage))
    #expect(app.account.stage == .awaitingBrowser(URL(string: page)))
    #expect(record.opened == [URL(string: page)])
    app.dismissSignIn()
    #expect(app.signInPrompt == "Continue signing in")
    app.beginSignIn()
    #expect(record.opened.count == 1)
    app.openSignInPage()
    #expect(record.opened.count == 2)
    #expect(record.sent.filter { $0 == "auth.login.start system" }.count == 1)
    #expect(record.raises.value == 0)
    try app.receive(runtimeEvent("auth.finalizing", epoch: 0, userId: nil))
    #expect(app.account.stage == .finalizing)
    #expect(record.raises.value == 1)
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    #expect(app.account.stage == .signedIn)
    #expect(app.account.localDeviceEnrolled)
    #expect(app.signInPrompt == nil)
    app.dismissSignIn()
    #expect(!app.signInPresented)
    #expect(app.account.stage == .idle)
    app.shutdownProcess()
}

@Test @MainActor func `cancel stops the pending sign in and failures keep progress`() throws {
    let (app, record) = try signedOutApp(prefix: "SignInCancel")
    app.beginSignIn()
    try app.receive(runtimeEvent("auth.browser", epoch: 0, userId: nil, fields: browserPage))
    app.cancelSignIn()
    #expect(record.sent.last == "auth.login.cancel")
    #expect(app.account.stage == .idle)
    #expect(!app.signInPresented)
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("cancelled")]))
    #expect(app.account.stage == .idle)
    app.beginSignIn()
    try app.receive(runtimeEvent("auth.browser", epoch: 0, userId: nil, fields: browserPage))
    try app.receive(runtimeEvent("auth.error", epoch: 0, userId: nil, fields: ["message": .string("The sign-in took too long. Try again.")]))
    #expect(app.account.stage == .failed(message: "The sign-in took too long. Try again.", completed: 1))
    #expect(app.errorMessage == nil)
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.account.stage == .failed(message: "The sign-in took too long. Try again.", completed: 1))
    app.dismissSignIn()
    #expect(app.account.stage == .idle)
    app.beginSignIn()
    app.dismissSignIn()
    try app.receive(runtimeEvent("auth.error", epoch: 0, userId: nil, fields: ["message": .string("Backend request failed (401): nope")]))
    #expect(app.errorMessage == "Backend request failed (401): nope")
    app.shutdownProcess()
}

@Test @MainActor func `untrusted Mac continues into the trust step with approval or reset`() throws {
    let (app, record) = try signedOutApp(prefix: "SignInTrust")
    app.beginSignIn()
    try app.receive(runtimeEvent("auth.browser", epoch: 0, userId: nil, fields: browserPage))
    app.dismissSignIn()
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    #expect(app.account.stage == .trustingDevice(.choose))
    #expect(app.signInPresented)
    #expect(!app.account.localDeviceEnrolled)
    #expect(app.signInPrompt == "Trust this Mac")
    app.account.requestDeviceApproval()
    #expect(record.sent.last == "devices.link.startSelf")
    try app.receive(runtimeEvent("devices.result", fields: [
        "operation": .string("devices.link.startSelf"), "requestId": .string(UUIDv7.generate()), "code": .string("JKMN-PQRS-TVWX"),
    ]))
    #expect(app.account.stage == .trustingDevice(.pendingApproval(code: "JKMN-PQRS-TVWX")))
    try app.receive(runtimeEvent("devices.link.selfResolved", fields: ["outcome": .string("expired")]))
    #expect(app.account.stage == .trustingDevice(.choose))
    app.cancelSignIn()
    #expect(record.sent.last != "auth.login.cancel")
    #expect(!app.signInPresented)
    #expect(app.account.stage == .trustingDevice(.choose))
    app.beginSignIn()
    app.account.startFresh()
    #expect(record.sent.last == "devices.reset")
    try app.receive(runtimeEvent("devices.error", fields: ["message": .string("Sign in again to start fresh on this device.")]))
    #expect(app.account.stage == .trustingDevice(.failed("Sign in again to start fresh on this device.")))
    #expect(app.errorMessage == nil)
    app.account.chooseTrust()
    app.account.startFresh()
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    #expect(app.account.stage == .signedIn)
    #expect(app.account.localDeviceEnrolled)
    app.shutdownProcess()
}

@Test @MainActor func `restored untrusted account offers trust without stealing focus`() throws {
    let (app, _) = try signedOutApp(prefix: "SignInRestore")
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    #expect(app.account.stage == .trustingDevice(.choose))
    #expect(!app.signInPresented)
    #expect(app.signInPrompt == "Trust this Mac")
    #expect(app.deviceTrust == .choose)
    app.beginSignIn()
    #expect(app.signInPresented)
    #expect(app.account.stage == .trustingDevice(.choose))
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.account.stage == .idle)
    #expect(app.signInPrompt == "Sign in")
    app.shutdownProcess()
}

@Test @MainActor func `device notices lead to the account settings until dismissed and a new request shows again`() throws {
    let (app, _) = try signedOutApp(prefix: "DeviceNotice")
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    #expect(app.deviceNotice == .approvalNeeded)
    app.beginSignIn()
    #expect(app.deviceNotice == nil)
    app.dismissSignIn()
    app.account.requestDeviceApproval()
    try app.receive(runtimeEvent("devices.result", fields: [
        "operation": .string("devices.link.startSelf"), "requestId": .string(UUIDv7.generate()), "code": .string("JKMN-PQRS-TVWX"),
    ]))
    #expect(app.deviceNotice == .approvalWaiting)
    app.openDeviceNotice()
    #expect(app.workbench.section == .settings && app.workbench.settingsSection == .devices)
    #expect(!app.workbench.focusesDeviceCode)
    #expect(app.deviceNotice == nil)
    app.workbench.section = .sessions
    app.dismissDeviceNotice()
    #expect(app.deviceNotice == nil)

    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    let laptop: JSONValue = .object(["requestId": .string("laptop"), "deviceLabel": .string("Laptop")])
    let desktop: JSONValue = .object(["requestId": .string("desktop"), "deviceLabel": .string("Desktop")])
    try app.receive(runtimeEvent("devices.link.snapshot", fields: ["requests": .array([laptop])]))
    #expect(requestIds(app.deviceNotice) == ["laptop"])
    app.dismissDeviceNotice()
    #expect(app.deviceNotice == nil)
    try app.receive(runtimeEvent("devices.link.snapshot", fields: ["requests": .array([laptop, desktop])]))
    #expect(requestIds(app.deviceNotice) == ["desktop"])
    app.openDeviceNotice()
    #expect(app.workbench.focusesDeviceCode)
    #expect(app.deviceNotice == nil)
    app.shutdownProcess()
}

private func requestIds(_ notice: DeviceNotice?) -> [String]? {
    guard case let .requests(requests) = notice else { return nil }
    return requests.map(\.id)
}

@Test @MainActor func `trust reason shows until this Mac is approved and an ended sign in says so`() throws {
    let (app, _) = try signedOutApp(prefix: "SignInReason")
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    try app.receive(runtimeEvent("devices.list", fields: [
        "selfDeviceId": .string("this"), "localDeviceEnrolled": .bool(false), "devices": .array([]),
        "notice": .string("Approve this device from one of your existing devices."),
    ]))
    #expect(app.account.identityMessage == "Approve this device from one of your existing devices.")
    #expect(!app.account.isReady)
    try app.receive(runtimeEvent("devices.list", fields: [
        "selfDeviceId": .string("this"), "localDeviceEnrolled": .bool(true), "devices": .array([]),
    ]))
    #expect(app.account.identityMessage == nil)
    #expect(app.account.isReady)
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("expired")]))
    #expect(app.account.userId == nil)
    #expect(app.signInPrompt == "Sign in")
    #expect(app.errorMessage == "Your sign-in has ended. Sign in again.")
    app.shutdownProcess()
}

@Test @MainActor func `deleting the account waits for the browser and ends signed out with a note`() throws {
    let (app, record) = try signedOutApp(prefix: "AccountDeletion")
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    let page = "https://auth.example/realms/kodosi/protocol/openid-connect/auth?kc_action=delete_account"
    try app.receive(runtimeEvent("auth.deletion_pending", fields: ["confirmationUri": .string(page)]))
    #expect(app.account.deletion == .confirming(URL(string: page)))
    #expect(record.opened == [URL(string: page)])
    app.account.cancelAccountDeletion()
    #expect(app.account.deletion == nil)
    #expect(record.sent.last == "auth.deleteAccount.cancel")
    try app.receive(runtimeEvent("auth.deletion_pending", fields: ["confirmationUri": .string(page)]))
    try app.receive(runtimeEvent("auth.error", fields: [
        "operation": .string("auth.deleteAccount"), "message": .string("Kodosi stopped waiting for the confirmation."),
    ]))
    #expect(app.account.deletion == nil)
    #expect(app.account.stage == .signedIn)
    #expect(app.errorMessage == "Kodosi stopped waiting for the confirmation.")
    try app.receive(runtimeEvent("auth.deletion_pending", fields: ["confirmationUri": .string(page)]))
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("accountDeleted")]))
    #expect(app.account.deletion == nil)
    #expect(app.account.userId == nil)
    #expect(app.errorMessage == "Your Kodosi account is deleted.")
    app.shutdownProcess()
}
