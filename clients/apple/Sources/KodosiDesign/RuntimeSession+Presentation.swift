import Foundation
import KodosiBridge

public extension RuntimeSession {
    var alert: String? {
        guard canCall, waitState != nil else { return nil }
        return programStatus?.caption ?? sign(unseen: true)?.label
    }

    func mark(rested: AgentMark.Activity) -> AgentMark.Activity {
        if let progress {
            return .progress(progress)
        }
        return isWorking ? .working : rested
    }

    func sign(unseen: Bool) -> StatusSign.Form? {
        switch waitState {
        case .blocked:
            switch programStatus?.kind {
            case .question: .question
            case .auth: .key
            case .permission, nil: .hand
            }
        case .done where unseen: .done
        case .error where unseen: .failed
        default: unseen ? .changed : nil
        }
    }

    var agent: AgentKind {
        AgentKind(program: program)
    }

    var placeLabel: String? {
        guard let place else { return nil }
        return switch place.kind {
        case .remote: String(localized: "On \(place.name)", bundle: .module)
        case .container: String(localized: "In the container \(place.name)", bundle: .module)
        case .vm: String(localized: "In the virtual machine \(place.name)", bundle: .module)
        }
    }
}

public extension AgentMark {
    init(session: RuntimeSession, size: CGFloat = 22, activity: Activity = .awake) {
        self.init(kind: session.agent, size: size, activity: activity, root: session.runsAsRoot)
    }
}
