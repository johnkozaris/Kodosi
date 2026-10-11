import Foundation
import Observation

@Observable
final class NoticeCenter {
    struct Notice: Identifiable, Equatable {
        let id = UUID()
        let message: String
    }

    private(set) var current: Notice?

    func show(_ message: String) {
        current = Notice(message: message)
    }

    func show(_ error: any Error) {
        guard !(error is CancellationError) else { return }
        show(error.localizedDescription)
    }

    func dismiss(_ notice: Notice) {
        if current == notice {
            current = nil
        }
    }
}
