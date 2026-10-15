import Foundation

public enum SignInStage: Equatable, Sendable {
    case idle
    case starting
    case awaitingBrowser(URL?)
    case finalizing
    case trustingDevice(DeviceTrust)
    case signedIn
    case failed(message: String, completed: Int)

    public var isPending: Bool {
        switch self {
        case .starting, .awaitingBrowser, .finalizing, .trustingDevice: true
        case .idle, .signedIn, .failed: false
        }
    }

    public var completedSteps: Int {
        switch self {
        case .idle, .starting: 0
        case .awaitingBrowser: 1
        case .finalizing, .trustingDevice: 2
        case .signedIn: 3
        case let .failed(_, completed): completed
        }
    }
}

public enum DeviceTrust: Equatable, Sendable {
    case choose
    case requesting
    case pendingApproval(code: String)
    case recovering
    case resetting
    case failed(String)
}
