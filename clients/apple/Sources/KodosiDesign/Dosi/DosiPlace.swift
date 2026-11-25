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

    /// `terminal` and `project` resolve what Dosi names. `sessions` and `projects` should preserve
    /// their display order. When `openSettings` is `nil`, sign-in notices omit the settings button.
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
