import Foundation

struct RuntimeHandleSnapshot: @unchecked Sendable {
    let pointer: UnsafeMutableRawPointer
}

final class RuntimeHandleSlot: @unchecked Sendable {
    private let lock = NSLock()
    private var pointer: UnsafeMutableRawPointer?
    private var generation: UInt64 = 0

    var isRunning: Bool {
        lock.withLock { pointer != nil }
    }

    func currentGeneration() -> UInt64? {
        lock.withLock {
            guard pointer != nil else { return nil }
            return generation
        }
    }

    @discardableResult
    func install(_ pointer: UnsafeMutableRawPointer) -> UInt64 {
        lock.withLock {
            generation &+= 1
            self.pointer = pointer
            return generation
        }
    }

    func snapshot(matching expectedGeneration: UInt64) -> RuntimeHandleSnapshot? {
        lock.withLock {
            guard let pointer, generation == expectedGeneration else { return nil }
            return RuntimeHandleSnapshot(pointer: pointer)
        }
    }

    func tombstone() -> RuntimeHandleSnapshot? {
        lock.withLock {
            guard let pointer else { return nil }
            self.pointer = nil
            return RuntimeHandleSnapshot(pointer: pointer)
        }
    }
}
