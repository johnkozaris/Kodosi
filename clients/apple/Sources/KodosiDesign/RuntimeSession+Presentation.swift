import KodosiBridge

public extension RuntimeSession {
    var alert: String? {
        guard isOwner, waitState != nil else { return nil }
        return programStatus?.caption ?? sign(unseen: true)?.label
    }

    func mark(rested: AgentMark.Activity) -> AgentMark.Activity {
        if let progress {
            return .progress(progress)
        }
        if isWorking {
            return .working
        }
        return needsUser ? .asks : rested
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
}
