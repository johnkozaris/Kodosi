import Foundation
@testable import KodosiDesktop
import Testing

@MainActor
private func signedOutApp(prefix: String) throws -> (AppDependencies, SignInRecord) {
    let defaults = try #require(EphemeralUserDefaults(prefix: prefix))
    let bootstrap = try RuntimeStorageBootstrap.resolve(environment: [:], installEnvironment: false)
    let app = AppDependencies(defaults: defaults, storageBootstrap: bootstrap)
    let sent = Box<[String]>([])
    let opened = Box<[URL]>([])
    app.commandSink.transport = { data in
        if let command = try? JSONDecoder().decode([String: JSONValue].self, from: data), let type = command["type"]?.stringValue {
            sent.value.append(type)
        }
        return 0
    }
    app.openExternalURL = { opened.value.append($0) }
    try app.receive(runtimeEvent("system.ready", epoch: 0, userId: nil, fields: ["protocolVersion": .int(54)]))
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("signedOut")]))
    return (app, SignInRecord(sentCommands: sent, openedPages: opened))
}

private struct SignInRecord {
    let sentCommands: Box<[String]>
    let openedPages: Box<[URL]>
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

private let deviceCode: [String: JSONValue] = [
    "userCode": .string("ABCD-EFGH"), "verificationUri": .string("https://auth.example/device?user_code=ABCD-EFGH"),
]

@Test @MainActor func signInWalksThroughTheStepsAndOnlyOpensTheBrowserOnce() throws {
    let (app, record) = try signedOutApp(prefix: "SignInSteps")
    #expect(app.signInPrompt == "Sign in")
    app.beginSignIn()
    #expect(app.signInPresented)
    #expect(app.signInStage == .starting)
    #expect(record.sent.last == "auth.login.start")
    try app.receive(runtimeEvent("auth.device_code", epoch: 0, userId: nil, fields: deviceCode))
    #expect(app.signInStage == .awaitingApproval(code: "ABCD-EFGH", url: URL(string: "https://auth.example/device?user_code=ABCD-EFGH")))
    #expect(record.opened.count == 1)
    app.dismissSignIn()
    #expect(app.signInPrompt == "Continue signing in")
    app.beginSignIn()
    #expect(record.opened.count == 1)
    app.openSignInPage()
    #expect(record.opened.count == 2)
    #expect(record.sent.filter { $0 == "auth.login.start" }.count == 1)
    try app.receive(runtimeEvent("auth.finalizing", epoch: 0, userId: nil))
    #expect(app.signInStage == .finalizing)
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    #expect(app.signInStage == .signedIn)
    #expect(app.localDeviceEnrolled)
    #expect(app.signInPrompt == nil)
    app.dismissSignIn()
    #expect(!app.signInPresented)
    #expect(app.signInStage == .idle)
    app.shutdownProcess()
}

@Test @MainActor func cancelStopsThePendingSignInAndFailuresKeepProgress() throws {
    let (app, record) = try signedOutApp(prefix: "SignInCancel")
    app.beginSignIn()
    try app.receive(runtimeEvent("auth.device_code", epoch: 0, userId: nil, fields: deviceCode))
    app.cancelSignIn()
    #expect(record.sent.last == "auth.login.cancel")
    #expect(app.signInStage == .idle)
    #expect(!app.signInPresented)
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("cancelled")]))
    #expect(app.signInStage == .idle)
    app.beginSignIn()
    try app.receive(runtimeEvent("auth.device_code", epoch: 0, userId: nil, fields: deviceCode))
    try app.receive(runtimeEvent("auth.error", epoch: 0, userId: nil, fields: ["message": .string("Sign-in code expired.")]))
    #expect(app.signInStage == .failed(message: "Sign-in code expired.", completed: 1))
    #expect(app.errorMessage == nil)
    try app.receive(runtimeEvent("auth.required", epoch: 0, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.signInStage == .failed(message: "Sign-in code expired.", completed: 1))
    app.dismissSignIn()
    #expect(app.signInStage == .idle)
    app.beginSignIn()
    app.dismissSignIn()
    try app.receive(runtimeEvent("auth.error", epoch: 0, userId: nil, fields: ["message": .string("Backend request failed (401): nope")]))
    #expect(app.errorMessage == "Backend request failed (401): nope")
    app.shutdownProcess()
}

@Test @MainActor func untrustedMacContinuesIntoTheTrustStepWithApprovalOrReset() throws {
    let (app, record) = try signedOutApp(prefix: "SignInTrust")
    app.beginSignIn()
    try app.receive(runtimeEvent("auth.device_code", epoch: 0, userId: nil, fields: deviceCode))
    app.dismissSignIn()
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    #expect(app.signInStage == .trustingDevice(.choose))
    #expect(app.signInPresented)
    #expect(!app.localDeviceEnrolled)
    #expect(app.signInPrompt == "Trust this Mac")
    app.requestDeviceApproval()
    #expect(record.sent.last == "devices.link.startSelf")
    try app.receive(runtimeEvent("devices.link.selfPending", fields: ["code": .string("JKMN-PQRS-TVWX"), "expiresAt": .string("2026-09-17T01:00:00Z")]))
    #expect(app.signInStage == .trustingDevice(.pendingApproval(code: "JKMN-PQRS-TVWX")))
    try app.receive(runtimeEvent("devices.link.selfResolved", fields: ["outcome": .string("expired")]))
    #expect(app.signInStage == .trustingDevice(.choose))
    app.cancelSignIn()
    #expect(record.sent.last != "auth.login.cancel")
    #expect(!app.signInPresented)
    #expect(app.signInStage == .trustingDevice(.choose))
    app.beginSignIn()
    app.resetTrustedDevices()
    #expect(record.sent.last == "devices.reset")
    try app.receive(runtimeEvent("devices.error", fields: ["message": .string("Sign in again to start fresh on this device.")]))
    #expect(app.signInStage == .trustingDevice(.failed("Sign in again to start fresh on this device.")))
    #expect(app.errorMessage == nil)
    app.retryDeviceTrust()
    app.resetTrustedDevices()
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(true)]))
    #expect(app.signInStage == .signedIn)
    #expect(app.localDeviceEnrolled)
    app.shutdownProcess()
}

@Test @MainActor func restoredUntrustedAccountOffersTrustWithoutStealingFocus() throws {
    let (app, _) = try signedOutApp(prefix: "SignInRestore")
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    #expect(app.signInStage == .trustingDevice(.choose))
    #expect(!app.signInPresented)
    #expect(app.signInPrompt == "Trust this Mac")
    #expect(app.deviceTrust == .choose)
    app.beginSignIn()
    #expect(app.signInPresented)
    #expect(app.signInStage == .trustingDevice(.choose))
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("signedOut")]))
    #expect(app.signInStage == .idle)
    #expect(app.signInPrompt == "Sign in")
    app.shutdownProcess()
}

@Test @MainActor func trustReasonShowsUntilThisMacIsApprovedAndAnEndedSignInSaysSo() throws {
    let (app, _) = try signedOutApp(prefix: "SignInReason")
    try app.receive(runtimeEvent("auth.ready", fields: ["userId": .string("owner"), "enrolled": .bool(false)]))
    try app.receive(runtimeEvent("devices.list", fields: [
        "selfDeviceId": .string("this"), "localDeviceEnrolled": .bool(false), "devices": .array([]),
        "notice": .string("Approve this device from one of your existing devices."),
    ]))
    #expect(app.identityMessage == "Approve this device from one of your existing devices.")
    #expect(!app.accountReady)
    try app.receive(runtimeEvent("devices.list", fields: [
        "selfDeviceId": .string("this"), "localDeviceEnrolled": .bool(true), "devices": .array([]),
    ]))
    #expect(app.identityMessage == nil)
    #expect(app.accountReady)
    try app.receive(runtimeEvent("auth.required", epoch: 2, userId: nil, fields: ["reason": .string("expired")]))
    #expect(app.userId == nil)
    #expect(app.signInPrompt == "Sign in")
    #expect(app.errorMessage == "Your sign-in has ended. Sign in again.")
    app.shutdownProcess()
}
