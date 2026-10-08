import Foundation
import Synchronization

final class TerminalResizeAuthorityCell: Sendable {
    private let value: Mutex<Bool>

    init(_ value: Bool) {
        self.value = Mutex(value)
    }

    var isAllowed: Bool {
        value.withLock { $0 }
    }

    func update(_ newValue: Bool) {
        value.withLock { $0 = newValue }
    }
}
