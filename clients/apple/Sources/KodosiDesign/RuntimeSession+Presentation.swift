import Foundation
import KodosiBridge

public extension RuntimeSession {
    var alert: String? {
        programStatus?.caption ?? sign(unseen: true)?.label
    }

    func mark(rested: AgentMark.Activity) -> AgentMark.Activity {
        if let progress {
            return .progress(progress)
        }
        return working ? .working : rested
    }

    func sign(unseen: Bool) -> StatusSign.Form? {
        switch waiting {
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

    /// A list puts an agent's summary before the terminal's name only while the line below can
    /// still name the terminal, rather than say its state.
    func leadingSummary(_ summaryFirst: Bool) -> String? {
        guard summaryFirst, agent.isAgent, !isTroubled, connectionState != .connecting, status == .running else { return nil }
        return activity
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
        self.init(kind: session.agent, account: session.programAccount, size: size, activity: activity, root: session.runsAsRoot)
    }
}

public extension HelperMarks {
    init(session: RuntimeSession) {
        self.init(kind: session.agent, account: session.programAccount, count: session.workingHelpers)
    }
}
