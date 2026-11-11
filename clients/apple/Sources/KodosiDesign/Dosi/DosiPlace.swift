import KodosiBridge
import Observation

@MainActor
@Observable
public final class DosiPlace {
    public let dosi: DosiStore
    let terminal: (String) -> RuntimeSession?
    let project: (String) -> ProjectEntry?
    let sessions: () -> [RuntimeSession]
    let projects: () -> [ProjectEntry]
    let open: (DosiTarget) -> Void
    let answer: (DosiItem, [String]) -> Void
    let send: (String) -> Void
    let canSend: () -> Bool
    let openSettings: (() -> Void)?

    /// `terminal` and `project` find what Dosi names, and `sessions` and `projects` are the person's
    /// terminals and projects in the order of their lists. `send` sends a suggestion while `canSend`
    /// says so. Without `openSettings`, a notice that Dosi needs a sign-in has no key.
    public init(
        dosi: DosiStore,
        terminal: @escaping (String) -> RuntimeSession?,
        project: @escaping (String) -> ProjectEntry?,
        sessions: @escaping () -> [RuntimeSession],
        projects: @escaping () -> [ProjectEntry],
        open: @escaping (DosiTarget) -> Void,
        answer: @escaping (DosiItem, [String]) -> Void,
        send: @escaping (String) -> Void,
        canSend: @escaping () -> Bool,
        openSettings: (() -> Void)? = nil,
    ) {
        self.dosi = dosi
        self.terminal = terminal
        self.project = project
        self.sessions = sessions
        self.projects = projects
        self.open = open
        self.answer = answer
        self.send = send
        self.canSend = canSend
        self.openSettings = openSettings
    }
}
