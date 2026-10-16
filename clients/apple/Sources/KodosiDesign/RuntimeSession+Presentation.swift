import KodosiBridge

public extension RuntimeSession {
    var alert: String? {
        guard canCall, waitState != nil else { return nil }
        return programStatus?.caption ?? sign(unseen: true)?.label
    }

    func needsAttention(after previous: RuntimeSession?) -> Bool {
        guard canCall else { return false }
        let stopped = previous?.isWorking == true && !isWorking
        return stopped || waitState != nil && waitState != previous?.waitState
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
}
