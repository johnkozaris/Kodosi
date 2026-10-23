import Foundation
import Observation

@MainActor
@Observable
public final class AccountStore {
    public enum Deletion: Equatable, Sendable {
        case requested
        case confirming(URL?)
    }

    public enum Approval: Equatable, Sendable {
        case approving
        case approved(String)
        case failed(String)
    }

    public private(set) var userId: String?
    public private(set) var displayName: String?
    public private(set) var authResolved = false
    public private(set) var localDeviceEnrolled = false
    public private(set) var selfDeviceId: String?
    public private(set) var identityMessage: String?
    public private(set) var stage: SignInStage = .idle
    public private(set) var devices: [MyDeviceEntry] = []
    public private(set) var devicesRevision = 0
    public private(set) var onlineHosts: Set<String> = []
    public private(set) var deviceRequests: [DeviceLinkRequest] = []
    public private(set) var approval: Approval?
    public private(set) var creatingRecoveryKey = false
    public private(set) var newRecoveryKey: String?
    public private(set) var deletion: Deletion?
    public private(set) var deletionPage: URL?
    public private(set) var signInPage: URL?
    public private(set) var restoring: String?

    @ObservationIgnored public var notify: (String) -> Void = { _ in }
    @ObservationIgnored private let commands: CommandSink
    @ObservationIgnored private let browser: String

    public init(commands: CommandSink, browser: String) {
        self.commands = commands
        self.browser = browser
    }

    public var isReady: Bool {
        userId != nil && localDeviceEnrolled
    }

    public var needsOnboarding: Bool {
        authResolved && !isReady
    }

    public var selfName: String {
        let name = displayName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return name.isEmpty ? String(localized: "You", bundle: .module) : name
    }

    public var orderedDevices: [MyDeviceEntry] {
        devices.filter { $0.deviceId == selfDeviceId }
            + devices.filter { $0.deviceId != selfDeviceId && !$0.recoveryKey }
            + devices.filter(\.recoveryKey)
    }

    public var hasRecoveryKey: Bool {
        devices.contains(where: \.recoveryKey)
    }

    public var computers: [MyDeviceEntry] {
        devices.filter { onlineHosts.contains($0.deviceId) && $0.deviceId != selfDeviceId && !$0.recoveryKey }
            .sorted { $0.label.localizedStandardCompare($1.label) == .orderedAscending }
    }

    public func canRemove(_ device: MyDeviceEntry) -> Bool {
        localDeviceEnrolled && device.deviceId != selfDeviceId
    }

    public func deletionSummary(rooms: [MissionEntry], terminals: String) -> String {
        let owned = rooms.filter { $0.ownerUserId == userId }.map(\.name)
        var parts = [String(localized: "Your devices, friends and sharing go away. What you wrote in other rooms stays there.", bundle: .module)]
        if !owned.isEmpty {
            parts.append(String(localized: "These rooms close for everyone in them: \(owned.formatted(.list(type: .and))).", bundle: .module))
        }
        parts.append(terminals)
        return parts.joined(separator: "\n\n")
    }

    public func reset(userId: String?) {
        self.userId = userId
        displayName = nil
        authResolved = false
        localDeviceEnrolled = false
        selfDeviceId = nil
        identityMessage = nil
        devices = []
        onlineHosts = []
        deviceRequests = []
        approval = nil
        creatingRecoveryKey = false
        newRecoveryKey = nil
        deletion = nil
        deletionPage = nil
        signInPage = nil
        restoring = nil
        if !stage.isPending {
            stage = .idle
        }
    }

    public func receiveAuth(_ event: RuntimeEvent) throws {
        if event.type != "auth.restoring", event.type != "auth.deletion_pending" {
            restoring = nil
        }
        switch event.type {
        case "auth.ready": try receiveReady(event)
        case "auth.required": receiveRequired(event)
        case "auth.restoring": restoring = event.string("reason")
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

    public func receiveDevices(_ event: RuntimeEvent) throws {
        switch event.type {
        case "devices.list": try receiveList(event)
        case "devices.link.snapshot":
            deviceRequests = try event.value("requests")
        case "devices.hosts":
            onlineHosts = try Set(event.value("deviceIds", as: [String].self))
        case "devices.result" where event.string("operation") == "devices.link.startSelf":
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
        case "devices.result" where event.string("operation") == "devices.recovery.create":
            newRecoveryKey = event.string("key")
            creatingRecoveryKey = false
        default:
            break
        }
    }

    @discardableResult
    public func absorb(error event: RuntimeEvent, message: String) -> Bool {
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

    private func receiveList(_ event: RuntimeEvent) throws {
        selfDeviceId = event.string("selfDeviceId")
        localDeviceEnrolled = try event.value("localDeviceEnrolled")
        identityMessage = localDeviceEnrolled ? nil : event.string("notice")
        devices = try event.value("devices")
        devicesRevision += 1
        if localDeviceEnrolled, case .trustingDevice = stage {
            stage = .signedIn
        }
    }

    private func receiveReady(_ event: RuntimeEvent) throws {
        guard event.string("userId") == userId else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got an answer for a different account.", bundle: .module))
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
        case "expired": notify(String(localized: "Your sign-in has ended. Sign in again.", bundle: .module))
        case "accountDeleted": notify(String(localized: "Your Kodosi account is deleted.", bundle: .module))
        default: break
        }
    }

    public func signIn() {
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

    public func restartSignIn() {
        stage = .starting
        commands.perform("auth.login.start", ["browser": .string(browser)])
    }

    public func finishSignIn(with answer: URL) {
        guard case .awaitingBrowser = stage else { return }
        signInPage = nil
        commands.perform("auth.login.finish", ["url": .string(answer.absoluteString)])
    }

    public func cancelSignIn() {
        switch stage {
        case .starting, .awaitingBrowser, .finalizing:
            commands.perform("auth.login.cancel")
            stage = .idle
            signInPage = nil
        case .trustingDevice, .idle, .signedIn, .failed:
            break
        }
    }

    public func closeSignIn() {
        if !stage.isPending {
            stage = .idle
        }
    }

    public func endSignIn() {
        stage = .idle
        signInPage = nil
    }

    public func failSignIn(_ message: String) {
        commands.perform("auth.login.cancel")
        stage = .failed(message: message, completed: 0)
        signInPage = nil
    }

    public func requestDeviceApproval() {
        stage = .trustingDevice(.requesting)
        commands.perform("devices.link.startSelf")
    }

    public func cancelDeviceApproval() {
        commands.perform("devices.link.cancelSelf")
        stage = .trustingDevice(.choose)
    }

    public func useRecoveryKey(_ key: String) {
        let key = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !key.isEmpty else { return }
        stage = .trustingDevice(.recovering)
        commands.perform("devices.recovery.use", ["key": .string(key)])
    }

    public func startFresh() {
        stage = .trustingDevice(.resetting)
        commands.perform("devices.reset")
    }

    public func chooseTrust() {
        stage = .trustingDevice(.choose)
    }

    public func signOut() {
        commands.perform("auth.logout")
    }

    public func refreshDevices() async {
        let revision = devicesRevision
        commands.perform("devices.refresh")
        await RefreshWait.until { devicesRevision != revision }
    }

    public func approveDevice(code: String) {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, approval != .approving else { return }
        approval = .approving
        commands.perform("devices.link.approve", ["code": .string(code)])
    }

    public func finishApproval() {
        approval = nil
    }

    public func revokeDevice(_ device: MyDeviceEntry) {
        commands.perform("devices.revoke", ["deviceId": .string(device.deviceId)])
    }

    public func createRecoveryKey() {
        guard !creatingRecoveryKey else { return }
        creatingRecoveryKey = true
        commands.perform("devices.recovery.create")
    }

    public func dismissRecoveryKey() {
        newRecoveryKey = nil
    }

    public func deleteAccount() {
        guard deletion == nil else { return }
        deletion = .requested
        commands.perform("auth.deleteAccount")
    }

    public func cancelAccountDeletion() {
        deletion = nil
        deletionPage = nil
        commands.perform("auth.deleteAccount.cancel")
    }

    public func closeDeletionPage() {
        deletionPage = nil
    }

    public func reopenDeletionPage() {
        if case let .confirming(page) = deletion {
            deletionPage = page
        }
    }
}
