import Foundation
import GhosttyKit

final class InMemoryTerminalSurfaceAccess: @unchecked Sendable {
    typealias Write = @Sendable (ghostty_surface_t, Data) -> Void
    typealias Restore = @Sendable (ghostty_surface_t, Data) -> Bool
    typealias ProcessExit = @Sendable (ghostty_surface_t, UInt32, UInt64) -> Void

    static let maximumPendingWriteBytes = 8 * 1024 * 1024
    private static let maximumPendingWrites = 256

    private let condition = NSCondition()
    private let outputQueue: DispatchQueue
    private let outputQueueKey = DispatchSpecificKey<UInt8>()
    private let write: Write
    private let restore: Restore
    private let processExit: ProcessExit

    private var surface: ghostty_surface_t?
    private var generation: UInt64 = 0
    private var activeOperations = 0
    private var pendingOperations: [Data] = []
    private var pendingWriteBytes = 0
    private var queuedWriteBytes = 0
    private var queuedWrites = 0
    private var discardsWritesUntilReplacement = false

    init(
        write: @escaping Write,
        restore: @escaping Restore,
        processExit: @escaping ProcessExit
    ) {
        outputQueue = DispatchQueue(
            label: "com.lakr233.libghostty-spm.in-memory-output",
            qos: .userInitiated
        )
        outputQueue.setSpecific(key: outputQueueKey, value: 1)
        self.write = write
        self.restore = restore
        self.processExit = processExit
    }

    func setSurface(_ surface: ghostty_surface_t?) {
        condition.lock()
        generation &+= 1
        self.surface = nil
        waitForActiveOperations()
        self.surface = surface
        let pendingOperations = surface == nil ? [] : takePendingOperations()
        let generation = generation
        for operation in pendingOperations {
            enqueue(operation, generation: generation)
        }
        condition.unlock()
    }

    @discardableResult
    func clearSurface(ifMatches expectedSurface: ghostty_surface_t?) -> Bool {
        condition.lock()
        guard surface == expectedSurface else {
            condition.unlock()
            return false
        }

        generation &+= 1
        pendingOperations.removeAll(keepingCapacity: false)
        pendingWriteBytes = 0
        surface = nil
        waitForActiveOperations()
        condition.unlock()
        return true
    }

    var currentSurface: ghostty_surface_t? {
        condition.lock()
        defer { condition.unlock() }
        return surface
    }

    func restoreCheckpointSynchronously(_ data: Data) -> Bool {
        guard data.count <= Self.maximumPendingWriteBytes,
              DispatchQueue.getSpecific(key: outputQueueKey) == nil
        else { return false }

        return outputQueue.sync { [self] in
            let accepted = withCurrentSurface { surface in restore(surface, data) } ?? false
            if accepted {
                condition.lock()
                discardsWritesUntilReplacement = false
                condition.unlock()
            }
            return accepted
        }
    }

    @discardableResult
    func enqueueWrite(_ data: Data) -> Bool {
        condition.lock()
        guard !discardsWritesUntilReplacement else {
            condition.unlock()
            return false
        }
        guard data.count <= Self.maximumPendingWriteBytes,
              queuedWriteBytes + pendingWriteBytes <= Self.maximumPendingWriteBytes - data.count,
              queuedWrites + pendingOperations.count < Self.maximumPendingWrites
        else {
            pendingOperations.removeAll()
            pendingWriteBytes = 0
            discardsWritesUntilReplacement = true
            condition.unlock()
            return false
        }
        guard !data.isEmpty else {
            condition.unlock()
            return true
        }
        guard surface != nil else {
            pendingOperations.append(data)
            pendingWriteBytes += data.count
            condition.unlock()
            return true
        }
        enqueue(data, generation: generation)
        condition.unlock()
        return true
    }

    @discardableResult
    func enqueueProcessExit(
        exitCode: UInt32,
        runtimeMilliseconds: UInt64
    ) -> Bool {
        guard let generation = currentGeneration else { return false }
        outputQueue.async { [self] in
            withSurface(generation: generation) { surface in
                processExit(surface, exitCode, runtimeMilliseconds)
            }
        }
        return true
    }

    func withCurrentSurface<Result>(
        _ operation: (ghostty_surface_t) -> Result
    ) -> Result? {
        condition.lock()
        guard let surface else {
            condition.unlock()
            return nil
        }
        activeOperations += 1
        condition.unlock()

        defer { finishOperation() }
        return operation(surface)
    }

    func attemptRestoreFromOutputQueueForTesting(_ data: Data) -> Bool {
        outputQueue.sync { restoreCheckpointSynchronously(data) }
    }

    func waitForPendingOutput() {
        outputQueue.sync {}
    }

    private var currentGeneration: UInt64? {
        condition.lock()
        defer { condition.unlock() }
        return surface == nil ? nil : generation
    }

    private func enqueue(_ data: Data, generation: UInt64) {
        queuedWriteBytes += data.count
        queuedWrites += 1
        outputQueue.async { [self] in
            defer {
                condition.lock()
                queuedWriteBytes -= data.count
                queuedWrites -= 1
                condition.unlock()
            }
            withSurface(generation: generation) { surface in
                write(surface, data)
            }
        }
    }

    private func takePendingOperations() -> [Data] {
        defer {
            pendingOperations.removeAll(keepingCapacity: false)
            pendingWriteBytes = 0
        }
        return pendingOperations
    }

    private func withSurface(
        generation expectedGeneration: UInt64,
        _ operation: (ghostty_surface_t) -> Void
    ) {
        condition.lock()
        guard generation == expectedGeneration, let surface else {
            condition.unlock()
            return
        }
        activeOperations += 1
        condition.unlock()

        defer { finishOperation() }
        operation(surface)
    }

    private func finishOperation() {
        condition.lock()
        activeOperations -= 1
        if activeOperations == 0 {
            condition.broadcast()
        }
        condition.unlock()
    }

    private func waitForActiveOperations() {
        while activeOperations > 0 {
            condition.wait()
        }
    }
}
