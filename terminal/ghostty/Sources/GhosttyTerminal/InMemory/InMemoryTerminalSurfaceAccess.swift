import Foundation
import GhosttyKit

final class InMemoryTerminalSurfaceAccess: @unchecked Sendable {
    typealias Write = @Sendable (ghostty_surface_t, Data) -> Void
    typealias HostResize = @Sendable (ghostty_surface_t, InMemoryTerminalViewport) -> Bool
    typealias Restore = @Sendable (ghostty_surface_t, Data, Bool) -> Bool

    /// The host's output and its resizes apply in the order they arrive.
    private enum Operation {
        case write(Data)
        case hostResize(InMemoryTerminalViewport)

        var byteCount: Int {
            guard case let .write(data) = self else { return 0 }
            return data.count
        }
    }

    static let maximumPendingWriteBytes = 8 * 1024 * 1024
    private static let maximumPendingOperations = 256

    private let condition = NSCondition()
    private let outputQueue: DispatchQueue
    private let outputQueueKey = DispatchSpecificKey<UInt8>()
    private let write: Write
    private let hostResize: HostResize
    private let restore: Restore

    private var surface: ghostty_surface_t?
    private var generation: UInt64 = 0
    private var activeOperations = 0
    private var pendingOperations: [Operation] = []
    private var pendingWriteBytes = 0
    private var queuedWriteBytes = 0
    private var queuedOperations = 0
    private var discardsWritesUntilReplacement = false

    init(write: @escaping Write, hostResize: @escaping HostResize, restore: @escaping Restore) {
        outputQueue = DispatchQueue(
            label: "com.lakr233.libghostty-spm.in-memory-output",
            qos: .userInitiated
        )
        outputQueue.setSpecific(key: outputQueueKey, value: 1)
        self.write = write
        self.hostResize = hostResize
        self.restore = restore
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

    func restoreCheckpointSynchronously(_ data: Data, console: Bool) -> Bool {
        guard data.count <= Self.maximumPendingWriteBytes,
              DispatchQueue.getSpecific(key: outputQueueKey) == nil
        else { return false }

        return outputQueue.sync { [self] in
            let accepted = withCurrentSurface { surface in restore(surface, data, console) } ?? false
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
        admit(.write(data))
    }

    @discardableResult
    func enqueueHostResize(_ viewport: InMemoryTerminalViewport) -> Bool {
        admit(.hostResize(viewport))
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
        outputQueue.sync { restoreCheckpointSynchronously(data, console: false) }
    }

    func waitForPendingOutput() {
        outputQueue.sync {}
    }

    private func admit(_ operation: Operation) -> Bool {
        condition.lock()
        defer { condition.unlock() }
        guard !discardsWritesUntilReplacement else { return false }
        let bytes = operation.byteCount
        guard bytes <= Self.maximumPendingWriteBytes,
              queuedWriteBytes + pendingWriteBytes <= Self.maximumPendingWriteBytes - bytes,
              queuedOperations + pendingOperations.count < Self.maximumPendingOperations
        else {
            pendingOperations.removeAll()
            pendingWriteBytes = 0
            discardsWritesUntilReplacement = true
            return false
        }
        if case let .write(data) = operation, data.isEmpty {
            return true
        }
        guard surface != nil else {
            pendingOperations.append(operation)
            pendingWriteBytes += bytes
            return true
        }
        enqueue(operation, generation: generation)
        return true
    }

    private func enqueue(_ operation: Operation, generation: UInt64) {
        let bytes = operation.byteCount
        queuedWriteBytes += bytes
        queuedOperations += 1
        outputQueue.async { [self] in
            defer {
                condition.lock()
                queuedWriteBytes -= bytes
                queuedOperations -= 1
                condition.unlock()
            }
            withSurface(generation: generation) { surface in
                apply(operation, to: surface)
            }
        }
    }

    private func apply(_ operation: Operation, to surface: ghostty_surface_t) {
        switch operation {
        case let .write(data):
            write(surface, data)
        case let .hostResize(viewport):
            guard !hostResize(surface, viewport) else { return }
            // The terminal does not have the host's grid, so the output that
            // follows waits for a checkpoint.
            condition.lock()
            generation &+= 1
            discardsWritesUntilReplacement = true
            condition.unlock()
        }
    }

    private func takePendingOperations() -> [Operation] {
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
