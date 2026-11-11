import KodosiBridge
import Observation

@MainActor
@Observable
public final class DosiPlace {
    public let dosi: DosiStore
    let terminal: (String) -> RuntimeSession?
    let room: (String) -> MissionEntry?
    let sessions: () -> [RuntimeSession]
    let rooms: () -> [MissionEntry]
    let open: (DosiTarget) -> Void
    let answer: (DosiItem, [String]) -> Void
    let send: (String) -> Void
    let canSend: () -> Bool
    let openSettings: (() -> Void)?

    /// `terminal` and `room` find what Dosi names, and `sessions` and `rooms` are the person's
    /// terminals and rooms in the order of their lists. `send` sends a suggestion while `canSend`
    /// says so. Without `openSettings`, a notice that Dosi needs a sign-in has no key.
    public init(
        dosi: DosiStore,
        terminal: @escaping (String) -> RuntimeSession?,
        room: @escaping (String) -> MissionEntry?,
        sessions: @escaping () -> [RuntimeSession],
        rooms: @escaping () -> [MissionEntry],
        open: @escaping (DosiTarget) -> Void,
        answer: @escaping (DosiItem, [String]) -> Void,
        send: @escaping (String) -> Void,
        canSend: @escaping () -> Bool,
        openSettings: (() -> Void)? = nil,
    ) {
        self.dosi = dosi
        self.terminal = terminal
        self.room = room
        self.sessions = sessions
        self.rooms = rooms
        self.open = open
        self.answer = answer
        self.send = send
        self.canSend = canSend
        self.openSettings = openSettings
    }
}
