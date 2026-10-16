import Foundation
import KodosiBridge

extension AppDependencies {
    var deviceTrust: DeviceTrust {
        if case let .trustingDevice(trust) = account.stage {
            trust
        } else {
            .choose
        }
    }

    var signInPrompt: String? {
        switch account.stage {
        case .trustingDevice: String(localized: "Trust this Mac")
        case .signedIn: nil
        case .starting, .awaitingBrowser, .finalizing: String(localized: "Continue signing in")
        case .idle, .failed:
            if account.userId == nil {
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
