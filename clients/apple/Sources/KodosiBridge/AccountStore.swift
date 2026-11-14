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
        case checking
        case confirming(String)
        case approving
        case approved(String)
        case failed(String)

        public var isBusy: Bool {
            self == .checking || self == .approving
        }
    }

    public enum Offer: Equatable, Sendable {
        case requesting
        case shown(qr: [String])
        case approved(String)
        case ended
        case failed(String)
    }

    public private(set) var userId: String?
    public private(set) var displayName: String?
    public private(set) var authResolved = false
    public private(set) var localDeviceEnrolled = false
    public private(set) var selfDeviceId: String?
    public private(set) var stage: SignInStage = .idle
    public private(set) var devices: [MyDeviceEntry] = []
    public private(set) var devicesRevision = 0
    public private(set) var onlineHosts: Set<String> = []
    public private(set) var deviceRequests: [DeviceLinkRequest] = []
    public private(set) var pendingReset: DeviceReset?
    public private(set) var approval: Approval?
    public private(set) var offer: Offer?
    public private(set) var creatingRecoveryKey = false
    public private(set) var newRecoveryKey: String?
    public private(set) var deletion: Deletion?
    public private(set) var deletionPage: URL?
    public private(set) var signInPage: URL?
    public private(set) var restoring: String?
    public private(set) var credentialStore: String?
    public private(set) var credentialFolder: String?
    public private(set) var online = true

    @ObservationIgnored public var notify: (String) -> Void = { _ in }
    @ObservationIgnored private var checkedCode: String?
    @ObservationIgnored private var checkRequest: String?
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

    public var otherDeviceReset: DeviceReset? {
        pendingReset.flatMap { $0.thisDevice ? nil : $0 }
    }

    public var hasRecoveryKey: Bool {
        devices.contains(where: \.recoveryKey)
    }

    public var computers: [MyDeviceEntry] {
        devices.filter { onlineHosts.contains($0.deviceId) && $0.deviceId != selfDeviceId && !$0.recoveryKey }
            .sorted { $0.label.localizedStandardCompare($1.label) == .orderedAscending }
    }

    public var hasOtherDevices: Bool {
        devices.contains { $0.deviceId != selfDeviceId && !$0.recoveryKey }
    }

    public func receiveConnection(_ event: RuntimeEvent) throws {
        if event.type == "connection.state" {
            online = try event.value("online", as: Bool.self)
        }
    }

    public func retryConnection() {
        commands.perform("connection.retry")
    }

    public func canRemove(_ device: MyDeviceEntry) -> Bool {
        localDeviceEnrolled && device.deviceId != selfDeviceId
    }

    public func deletionSummary(projects: [ProjectEntry], terminals: String) -> String {
        let owned = projects.filter { $0.ownerUserId == userId }.map(\.name)
        var parts = [String(localized: "Kodosi deletes your devices, friends and sharing. What you wrote in other projects stays there.", bundle: .module)]
        if !owned.isEmpty {
            parts.append(String(localized: "These projects close for everyone in them: \(owned.formatted(.list(type: .and))).", bundle: .module))
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
        devices = []
        onlineHosts = []
        deviceRequests = []
        pendingReset = nil
        approval = nil
        offer = nil
        checkedCode = nil
        checkRequest = nil
        creatingRecoveryKey = false
        newRecoveryKey = nil
        deletion = nil
        deletionPage = nil
        signInPage = nil
        restoring = nil
        credentialStore = nil
        credentialFolder = nil
        online = true
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
        case "auth.restoring":
            restoring = event.string("reason")
            receivePlace(event)
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
            if let code = event.string("code") {
                stage = .trustingDevice(.pendingApproval(code: code, qr: Self.rows(of: event)))
            } else {
                stage = .trustingDevice(.joining)
            }
        case "devices.result" where event.string("operation") == "devices.link.offer":
            if offer == .requesting {
                offer = .shown(qr: Self.rows(of: event))
            }
        case "devices.link.offerResolved":
            guard case .shown = offer else { break }
            if event.string("outcome") == "approved" {
                offer = .approved(event.string("deviceLabel") ?? "")
            } else {
                offer = event.string("message").map(Offer.failed) ?? .ended
            }
        case "devices.link.selfResolved":
            if case .trustingDevice = stage, event.string("outcome") != "approved" {
                stage = .trustingDevice(.choose)
            }
            commands.perform("devices.refresh")
        case "devices.result" where event.string("operation") == "devices.link.check":
            if approval == .checking, event.string("requestId") == checkRequest {
                approval = .confirming(event.string("deviceLabel") ?? "")
            }
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
        case "devices.link.check" where approval == .checking && event.string("requestId") == checkRequest,
             "devices.link.approve" where approval == .approving:
            approval = .failed(message)
            return true
        case "devices.link.offer" where offer == .requesting:
            offer = .failed(message)
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
        devices = try event.value("devices")
        pendingReset = try event.payload["reset"].map { try $0.decode(as: DeviceReset?.self) } ?? nil
        devicesRevision += 1
        guard case let .trustingDevice(trust) = stage else { return }
        if localDeviceEnrolled {
            stage = .signedIn
        } else if let reset = pendingReset, reset.thisDevice {
            switch trust {
            case .choose, .resetting, .waiting: stage = .trustingDevice(.waiting(reset.readyAt))
            case .requesting, .pendingApproval, .joining, .recovering, .failed: break
            }
        } else if case .waiting = trust {
            stage = .trustingDevice(.choose)
        }
    }

    private static func rows(of event: RuntimeEvent) -> [String] {
        (try? event.value("qr", as: [String].self)) ?? []
    }

    private func receiveReady(_ event: RuntimeEvent) throws {
        guard event.string("userId") == userId else {
            throw RuntimeError.invalidResponse(String(localized: "Kodosi got an answer for a different account.", bundle: .module))
        }
        authResolved = true
        signInPage = nil
        receivePlace(event)
        if let name = event.string("displayName") {
            displayName = name
        }
        localDeviceEnrolled = event.payload["enrolled"] != .bool(false)
        if localDeviceEnrolled {
            stage = .signedIn
        } else if case .trustingDevice = stage {
        } else {
            stage = .trustingDevice(.choose)
        }
    }

    private func receivePlace(_ event: RuntimeEvent) {
        credentialStore = event.string("credentialStore")
        credentialFolder = event.string("credentialFolder")
    }

    private func receiveRequired(_ event: RuntimeEvent) {
        authResolved = true
        credentialStore = nil
        credentialFolder = nil
        deletion = nil
        deletionPage = nil
        signInPage = nil
        if case .failed = stage {} else {
            stage = .idle
        }
        switch event.string("reason") {
        case "expired": notify(String(localized: "Your sign-in has ended. Sign in again.", bundle: .module))
        case "accountDeleted": notify(String(localized: "Your Kodosi account is deleted.", bundle: .module))
        case "accountBanned":
            notify(String(
                localized: "This account is banned because it tried too many user names. Ask the operator of this Kodosi server to remove the ban.",
                bundle: .module,
            ))
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
        commands.perform("auth.login.start", ["browser": .string(browser), "language": .string(AppLanguage.current)])
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

    public func joinDevice(scanned code: String) {
        switch stage {
        case .trustingDevice(.choose), .trustingDevice(.pendingApproval), .trustingDevice(.failed):
            stage = .trustingDevice(.joining)
            commands.perform("devices.link.startSelf", ["code": .string(code)])
        default:
            break
        }
    }

    public func offerDevice() {
        offer = .requesting
        commands.perform("devices.link.offer")
    }

    public func endOffer() {
        switch offer {
        case .requesting, .shown: commands.perform("devices.link.offer.cancel")
        case .approved, .ended, .failed, nil: break
        }
        offer = nil
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

    public func stopReset() {
        guard let reset = pendingReset else { return }
        commands.perform("devices.reset.cancel", ["resetId": .string(reset.resetId)])
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

    public func checkDevice(code: String) {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, approval?.isBusy != true else { return }
        let request = UUIDv7.generate()
        checkedCode = code
        checkRequest = request
        approval = .checking
        commands.perform("devices.link.check", ["code": .string(code), "requestId": .string(request)])
    }

    public func confirmApproval() {
        guard case .confirming = approval, let code = checkedCode else { return }
        approval = .approving
        commands.perform("devices.link.approve", ["code": .string(code)])
    }

    public func finishApproval() {
        approval = nil
        checkedCode = nil
        checkRequest = nil
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
