import Foundation
import KodosiBridge
import Observation

@Observable
final class AccountStore {
    enum Deletion: Equatable {
        case requested
        case confirming(URL?)
    }

    enum Approval: Equatable {
        case approving
        case approved(String)
        case failed(String)
    }

    private(set) var userId: String?
    private(set) var displayName: String?
    private(set) var authResolved = false
    private(set) var localDeviceEnrolled = false
    private(set) var selfDeviceId: String?
    private(set) var identityMessage: String?
    private(set) var stage: SignInStage = .idle
    private(set) var devices: [MyDeviceEntry] = []
    private(set) var devicesRevision = 0
    private(set) var deviceRequests: [DeviceLinkRequest] = []
    private(set) var approval: Approval?
    private(set) var creatingRecoveryKey = false
    var newRecoveryKey: String?
    private(set) var deletion: Deletion?
    private(set) var deletionPage: URL?
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

    var orderedDevices: [MyDeviceEntry] {
        devices.filter { $0.deviceId == selfDeviceId }
            + devices.filter { $0.deviceId != selfDeviceId && !$0.recoveryKey }
            + devices.filter(\.recoveryKey)
    }

    var hasRecoveryKey: Bool {
        devices.contains(where: \.recoveryKey)
    }

    func canRemove(_ device: MyDeviceEntry) -> Bool {
        localDeviceEnrolled && device.deviceId != selfDeviceId
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
        approval = nil
        creatingRecoveryKey = false
        newRecoveryKey = nil
        deletion = nil
        deletionPage = nil
        signInPage = nil
        if !stage.isPending {
            stage = .idle
        }
    }

    func receiveAuth(_ event: RuntimeEvent) throws {
        switch event.type {
        case "auth.ready": try receiveReady(event)
        case "auth.required": receiveRequired(event)
        case "auth.browser":
            let url = event.string("url").flatMap(ExternalAuthURL.parse)
            stage = .awaitingBrowser(url)
            signInPage = url
        case "auth.finalizing":
            stage = .finalizing
            signInPage = nil
        case "auth.deletion_pending":
            let page = event.string("confirmationUri").flatMap(ExternalAuthURL.parse)
            deletion = .confirming(page)
            deletionPage = page
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
            devicesRevision += 1
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
            if approval == .approving, event.string("outcome") == "approved" {
                approval = .approved(event.string("deviceLabel") ?? "")
            }
            commands.perform("devices.refresh")
        case "devices.recovery.created":
            newRecoveryKey = event.string("key")
            creatingRecoveryKey = false
        default:
            break
        }
    }

    func absorb(error event: RuntimeEvent, message: String) -> Bool {
        switch event.string("operation") {
        case "auth.deleteAccount":
            deletion = nil
            deletionPage = nil
            return false
        case "devices.link.approve" where approval == .approving:
            approval = .failed(message)
            return true
        case "devices.recovery.create":
            creatingRecoveryKey = false
            return false
        default:
            break
        }
        if event.type.hasPrefix("auth.") {
            guard stage.isPending || !isReady else { return false }
            stage = .failed(message: message, completed: stage.completedSteps)
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
        deletionPage = nil
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
        case .starting, .awaitingBrowser, .finalizing, .trustingDevice:
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
        commands.perform("auth.login.start", ["browser": .string("app")])
    }

    func finishSignIn(with answer: URL) {
        guard case .awaitingBrowser = stage else { return }
        signInPage = nil
        commands.perform("auth.login.finish", ["url": .string(answer.absoluteString)])
    }

    func cancelSignIn() {
        switch stage {
        case .starting, .awaitingBrowser, .finalizing:
            commands.perform("auth.login.cancel")
            stage = .idle
            signInPage = nil
        case .trustingDevice, .idle, .signedIn, .failed:
            break
        }
    }

    func failSignIn(_ message: String) {
        commands.perform("auth.login.cancel")
        stage = .failed(message: message, completed: 0)
        signInPage = nil
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

    func refreshDevices() async {
        let revision = devicesRevision
        commands.perform("devices.refresh")
        await RefreshWait.until { devicesRevision != revision }
    }

    func approveDevice(code: String) {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, approval != .approving else { return }
        approval = .approving
        commands.perform("devices.link.approve", ["code": .string(code)])
    }

    func finishApproval() {
        approval = nil
    }

    func revokeDevice(_ device: MyDeviceEntry) {
        commands.perform("devices.revoke", ["deviceId": .string(device.deviceId)])
    }

    func createRecoveryKey() {
        guard !creatingRecoveryKey else { return }
        creatingRecoveryKey = true
        commands.perform("devices.recovery.create")
    }

    func dismissRecoveryKey() {
        newRecoveryKey = nil
    }

    func deleteAccount() {
        guard deletion == nil else { return }
        deletion = .requested
        commands.perform("auth.deleteAccount")
    }

    func cancelAccountDeletion() {
        deletion = nil
        deletionPage = nil
        commands.perform("auth.deleteAccount.cancel")
    }

    func closeDeletionPage() {
        deletionPage = nil
    }

    func reopenDeletionPage() {
        if case let .confirming(page) = deletion {
            deletionPage = page
        }
    }
}
