import AppKit
import Foundation

extension AppDependencies {
    enum SignInStage: Equatable {
        case idle
        case starting
        case awaitingApproval(code: String, url: URL?)
        case finalizing
        case trustingDevice(DeviceTrust)
        case signedIn
        case failed(message: String, completed: Int)

        var isPending: Bool {
            switch self {
            case .starting, .awaitingApproval, .finalizing, .trustingDevice: true
            case .idle, .signedIn, .failed: false
            }
        }

        var completedSteps: Int {
            switch self {
            case .idle, .starting: 0
            case .awaitingApproval: 1
            case .finalizing, .trustingDevice: 2
            case .signedIn: 3
            case let .failed(_, completed): completed
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

    var deviceTrust: DeviceTrust {
        if case let .trustingDevice(trust) = signInStage {
            trust
        } else {
            .choose
        }
    }

    var accountReady: Bool {
        userId != nil && localDeviceEnrolled
    }

    var signInPrompt: String? {
        switch signInStage {
        case .trustingDevice: String(localized: "Trust this Mac")
        case .signedIn: nil
        case .starting, .awaitingApproval, .finalizing: String(localized: "Continue signing in")
        case .idle, .failed:
            if userId == nil {
                String(localized: "Sign in")
            } else {
                localDeviceEnrolled ? nil : String(localized: "Trust this Mac")
            }
        }
    }

    func beginSignIn() {
        signInPresented = true
        switch signInStage {
        case .starting, .awaitingApproval, .finalizing, .trustingDevice:
            break
        case .idle, .failed, .signedIn:
            if userId != nil, !localDeviceEnrolled {
                signInStage = .trustingDevice(.choose)
            } else {
                restartSignIn()
            }
        }
    }

    func restartSignIn() {
        signInStage = .starting
        openedLoginCode = nil
        perform("auth.login.start")
    }

    func openSignInPage() {
        if case let .awaitingApproval(_, url) = signInStage, let url {
            openExternalURL(url)
        }
    }

    func cancelSignIn() {
        if case .trustingDevice = signInStage {} else if signInStage.isPending {
            perform("auth.login.cancel")
            signInStage = .idle
        }
        dismissSignIn()
    }

    func requestDeviceApproval() {
        signInStage = .trustingDevice(.requesting)
        perform("devices.link.startSelf")
    }

    func cancelDeviceApproval() {
        perform("devices.link.cancelSelf")
        signInStage = .trustingDevice(.choose)
    }

    func useRecoveryKey(_ key: String) {
        signInStage = .trustingDevice(.recovering)
        perform("devices.recovery.use", ["key": .string(key)])
    }

    func resetTrustedDevices() {
        signInStage = .trustingDevice(.resetting)
        perform("devices.reset")
    }

    func retryDeviceTrust() {
        signInStage = .trustingDevice(.choose)
    }

    func dismissSignIn() {
        signInPresented = false
        if !signInStage.isPending {
            signInStage = .idle
        }
    }
}
