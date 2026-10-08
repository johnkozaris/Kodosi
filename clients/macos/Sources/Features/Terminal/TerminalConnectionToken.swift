import Foundation

final class TerminalConnectionToken: @unchecked Sendable {
    private static let generationLock = NSLock()
    private nonisolated(unsafe) static var generationCounter: UInt64 = 0

    private let lock = NSLock()
    private var active = true
    private var inputAllowed = false
    let subscriptionId: String
    let subscriptionGeneration: UInt64

    init() {
        subscriptionId = UUIDv7.generate()
        subscriptionGeneration = Self.nextGeneration()
    }

    var isActive: Bool {
        lock.lock()
        defer { lock.unlock() }
        return active
    }

    var canSendInput: Bool {
        lock.withLock { active && inputAllowed }
    }

    func setInputAllowed(_ allowed: Bool) {
        lock.withLock { inputAllowed = allowed }
    }

    @discardableResult
    func deactivate() -> Bool {
        lock.withLock {
            let wasActive = active
            active = false
            return wasActive
        }
    }

    private static func nextGeneration() -> UInt64 {
        generationLock.withLock {
            precondition(generationCounter < UInt64.max, "terminal subscription generation exhausted")
            generationCounter += 1
            return generationCounter
        }
    }
}
