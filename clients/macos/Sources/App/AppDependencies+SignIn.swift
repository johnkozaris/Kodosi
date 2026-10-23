import Foundation
import KodosiBridge

enum DeviceNotice: Equatable {
    case requests([DeviceLinkRequest])
    case approvalNeeded
    case approvalWaiting
}

extension AppDependencies {
    var deviceTrust: DeviceTrust {
        if case let .trustingDevice(trust) = account.stage {
            trust
        } else {
            .choose
        }
    }

    var deviceNotice: DeviceNotice? {
        guard !signInPresented, workbench.section != .settings || workbench.settingsSection != .devices else { return nil }
        if account.isReady {
            let waiting = account.deviceRequests.filter { !workbench.dismissedDeviceRequests.contains($0.id) }
            return waiting.isEmpty ? nil : .requests(waiting)
        }
        guard account.userId != nil, account.authResolved, !workbench.approvalNoticeDismissed else { return nil }
        if case .pendingApproval = deviceTrust {
            return .approvalWaiting
        }
        return .approvalNeeded
    }

    func openDeviceNotice() {
        workbench.showAccount(focusingCode: account.isReady)
    }

    func dismissDeviceNotice() {
        if account.isReady {
            workbench.dismissedDeviceRequests = Set(account.deviceRequests.map(\.id))
        } else {
            workbench.approvalNoticeDismissed = true
        }
    }

    var signInPrompt: String? {
        switch account.stage {
        case .trustingDevice: String(localized: "Trust this Mac")
        case .signedIn: nil
        case .starting, .awaitingBrowser, .finalizing: String(localized: "Continue signing in")
        case .idle, .failed:
            if account.userId == nil, account.restoring != nil {
                String(localized: "Signing in…")
            } else if account.userId == nil {
                String(localized: "Sign in")
            } else {
                account.localDeviceEnrolled ? nil : String(localized: "Trust this Mac")
            }
        }
    }

    func beginSignIn() {
        signInPresented = true
        if !account.stage.isPending {
            openedSignInPage = nil
        }
        account.signIn()
    }

    func restartSignIn() {
        openedSignInPage = nil
        account.restartSignIn()
    }

    func openSignInPage() {
        if case let .awaitingBrowser(url) = account.stage, let url {
            openExternalURL(url)
        }
    }

    func cancelSignIn() {
        account.cancelSignIn()
        dismissSignIn()
    }

    func dismissSignIn() {
        signInPresented = false
        account.closeSignIn()
    }
}
