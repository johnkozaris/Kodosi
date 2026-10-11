import Foundation
import KodosiBridge
import Observation

@Observable
final class AccountStore {
    enum SignInStage: Equatable {
        case idle
        case starting
        case awaitingApproval(code: String, url: URL?)
        case finalizing
        case trustingDevice(DeviceTrust)
        case signedIn
        case failed(String)

        var isPending: Bool {
            switch self {
            case .starting, .awaitingApproval, .finalizing, .trustingDevice: true
            case .idle, .signedIn, .failed: false
            }
        }
    }

    enum DeviceTrust: Equatable {
        case choose
        case requesting
        case pendingApproval(code: String)
        case recovering
        case resetting
        case failed(String)
    }

    struct Deletion: Equatable {
        let page: URL?
    }

    private(set) var userId: String?
    private(set) var displayName: String?
    private(set) var authResolved = false
    private(set) var localDeviceEnrolled = false
    private(set) var selfDeviceId: String?
    private(set) var identityMessage: String?
    private(set) var stage: SignInStage = .idle
    private(set) var devices: [MyDeviceEntry] = []
    private(set) var deviceRequests: [DeviceLinkRequest] = []
    var newRecoveryKey: String?
    private(set) var deletion: Deletion?
    private(set) var signInPage: URL?

    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private let notices: NoticeCenter

    init(commands: CommandSink, notices: NoticeCenter) {
        self.commands = commands
        self.notices = notices
    }

    var isReady: Bool {
        userId != nil && localDeviceEnrolled
    }

    var needsOnboarding: Bool {
        authResolved && !isReady
    }

    var selfName: String {
        let name = displayName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return name.isEmpty ? String(localized: "You") : name
    }

    var thisDevice: MyDeviceEntry? {
        devices.first { $0.deviceId == selfDeviceId }
    }

    func reset(userId: String?) {
        self.userId = userId
        displayName = nil
        authResolved = false
        localDeviceEnrolled = false
        selfDeviceId = nil
        identityMessage = nil
        devices = []
        deviceRequests = []
        newRecoveryKey = nil
        deletion = nil
        signInPage = nil
        if !stage.isPending {
            stage = .idle
        }
    }

    func receiveAuth(_ event: RuntimeEvent) throws {
        switch event.type {
        case "auth.ready": try receiveReady(event)
        case "auth.required": receiveRequired(event)
        case "auth.device_code":
            let url = event.string("verificationUri").flatMap(ExternalAuthURL.parse)
            stage = .awaitingApproval(code: event.string("userCode") ?? "", url: url)
            signInPage = url
        case "auth.finalizing":
            stage = .finalizing
            signInPage = nil
        case "auth.deletion_pending":
            let page = event.string("confirmationUri").flatMap(ExternalAuthURL.parse)
            deletion = Deletion(page: page)
        default:
            break
        }
    }

    func receiveDevices(_ event: RuntimeEvent) throws {
        switch event.type {
        case "devices.list":
            selfDeviceId = event.string("selfDeviceId")
            localDeviceEnrolled = try event.value("localDeviceEnrolled")
            identityMessage = localDeviceEnrolled ? nil : event.string("notice")
            devices = try event.value("devices")
            if localDeviceEnrolled, case .trustingDevice = stage {
                stage = .signedIn
            }
        case "devices.link.snapshot":
            deviceRequests = try event.value("requests")
        case "devices.link.selfPending":
            stage = .trustingDevice(.pendingApproval(code: event.string("code") ?? ""))
        case "devices.link.selfResolved":
            if case .trustingDevice = stage, event.string("outcome") != "approved" {
                stage = .trustingDevice(.choose)
            }
            commands.perform("devices.refresh")
        case "devices.link.resolved":
            commands.perform("devices.refresh")
        case "devices.recovery.created":
            newRecoveryKey = event.string("key")
        default:
            break
        }
    }

    func absorb(error event: RuntimeEvent, message: String) -> Bool {
        if event.string("operation") == "auth.deleteAccount" {
            deletion = nil
            return false
        }
        if event.type.hasPrefix("auth.") {
            stage = .failed(message)
            signInPage = nil
            return needsOnboarding || userId == nil
        }
        if event.type.hasPrefix("devices."), case .trustingDevice = stage {
            stage = .trustingDevice(.failed(message))
            return true
        }
        return false
    }

    private func receiveReady(_ event: RuntimeEvent) throws {
        guard event.string("userId") == userId else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got an answer for a different account."))
        }
        authResolved = true
        signInPage = nil
        if let name = event.string("displayName") {
            displayName = name
        }
        localDeviceEnrolled = event.payload["enrolled"] != .bool(false)
        if localDeviceEnrolled {
            identityMessage = nil
            stage = .signedIn
        } else if case .trustingDevice = stage {
        } else {
            stage = .trustingDevice(.choose)
        }
    }

    private func receiveRequired(_ event: RuntimeEvent) {
        authResolved = true
        deletion = nil
        signInPage = nil
        if case .failed = stage {} else {
            stage = .idle
        }
        switch event.string("reason") {
        case "expired": notices.show(String(localized: "Your sign-in has ended. Sign in again."))
        case "accountDeleted": notices.show(String(localized: "Your Kodosi account is deleted."))
        default: break
        }
    }

    func signIn() {
        switch stage {
        case .starting, .awaitingApproval, .finalizing, .trustingDevice:
            return
        case .idle, .failed, .signedIn:
            if userId != nil, !localDeviceEnrolled {
                stage = .trustingDevice(.choose)
            } else {
                restartSignIn()
            }
        }
    }

    func restartSignIn() {
        stage = .starting
        commands.perform("auth.login.start")
    }

    func cancelSignIn() {
        switch stage {
        case .starting, .awaitingApproval, .finalizing:
            commands.perform("auth.login.cancel")
            stage = .idle
            signInPage = nil
        case .trustingDevice, .idle, .signedIn, .failed:
            break
        }
    }

    func closeSignInPage() {
        signInPage = nil
    }

    func reopenSignInPage() {
        if case let .awaitingApproval(_, url) = stage {
            signInPage = url
        }
    }

    func requestDeviceApproval() {
        stage = .trustingDevice(.requesting)
        commands.perform("devices.link.startSelf")
    }

    func cancelDeviceApproval() {
        commands.perform("devices.link.cancelSelf")
        stage = .trustingDevice(.choose)
    }

    func useRecoveryKey(_ key: String) {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        stage = .trustingDevice(.recovering)
        commands.perform("devices.recovery.use", ["key": .string(key)])
    }

    func startFresh() {
        stage = .trustingDevice(.resetting)
        commands.perform("devices.reset")
    }

    func chooseTrust() {
        stage = .trustingDevice(.choose)
    }

    func signOut() {
        commands.perform("auth.logout")
    }

    func approveDevice(code: String) {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else { return }
        commands.perform("devices.link.approve", ["code": .string(code)])
    }

    func revokeDevice(_ device: MyDeviceEntry) {
        commands.perform("devices.revoke", ["deviceId": .string(device.deviceId)])
    }

    func createRecoveryKey() {
        commands.perform("devices.recovery.create")
    }

    func deleteAccount() {
        commands.perform("auth.deleteAccount")
    }

    func cancelAccountDeletion() {
        deletion = nil
        commands.perform("auth.deleteAccount.cancel")
    }
}
