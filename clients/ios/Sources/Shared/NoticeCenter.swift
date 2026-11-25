import Foundation
import KodosiBridge
import Observation

@Observable
final class NoticeCenter {
    struct Notice: Identifiable, Equatable {
        let id = UUID()
        let message: String
        var done = false
    }

    private(set) var current: Notice?

    func show(_ message: String) {
        current = Notice(message: message)
    }

    func confirm(_ message: String) {
        current = Notice(message: message, done: true)
    }

    func show(_ error: any Error) {
        switch error {
        case is CancellationError, RuntimeError.refused: return
        default: show(error.localizedDescription)
        }
    }

    func dismiss(_ notice: Notice) {
        if current == notice {
            current = nil
        }
    }
}
