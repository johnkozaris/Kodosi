import Darwin
import Foundation
import GhosttyKit
@testable import GhosttyTerminal
import Testing

struct InMemoryTerminalSessionOutputQueueTests {
    @Test
    func `receive returns before surface write completes`() {
        let writeStarted = DispatchSemaphore(value: 0)
        let allowWriteToFinish = DispatchSemaphore(value: 0)
        let session = makeSession { _, _ in
            writeStarted.signal()
            allowWriteToFinish.wait()
        }
        session.setSurface(testSurface(1))

        DispatchQueue.global().asyncAfter(deadline: .now() + 1) {
            allowWriteToFinish.signal()
        }

        let start = ProcessInfo.processInfo.systemUptime
        session.receive(Data("hello".utf8))
        let elapsed = ProcessInfo.processInfo.systemUptime - start

        #expect(elapsed < 0.2)
        #expect(writeStarted.wait(timeout: .now() + 1) == .success)
        allowWriteToFinish.signal()
        session.waitForPendingOutput()
    }

    @Test
    func `attached output remains bounded until a fresh checkpoint`() {
        let started = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let session = makeSession { _, data in
            if data.count > 1 {
                started.signal()
                release.wait()
            }
        }
        session.setSurface(testSurface(21))
        let limit = InMemoryTerminalSurfaceAccess.maximumPendingWriteBytes
        #expect(session.receive(Data(repeating: 0x41, count: limit)))
        #expect(started.wait(timeout: .now() + 1) == .success)
        #expect(!session.receive(Data([0x42])))
        release.signal()
        session.waitForPendingOutput()
        #expect(!session.receive(Data([0x43])))
        #expect(session.restoreCheckpointSynchronously(Data([0x44])))
        #expect(session.receive(Data([0x45])))
        session.waitForPendingOutput()
    }

    @Test
    func `authoritative checkpoint restore waits for prior output and fences later output`() {
        let writeStarted = DispatchSemaphore(value: 0)
        let allowWriteToFinish = DispatchSemaphore(value: 0)
        let writes = LockedValues<String>()
        let session = makeSession(
            surfaceWrite: { _, data in
                let value = String(decoding: data, as: UTF8.self)
                writes.append(value)
                if value == "blocking" {
                    writeStarted.signal()
                    allowWriteToFinish.wait()
                }
            },
            surfaceRestore: { _, data in
                writes.append("restore:\(String(decoding: data, as: UTF8.self))")
                return true
            }
        )
        session.setSurface(testSurface(15))
        session.receive("blocking")
        #expect(writeStarted.wait(timeout: .now() + 1) == .success)
        DispatchQueue.global().asyncAfter(deadline: .now() + 1) {
            allowWriteToFinish.signal()
        }

        let start = ProcessInfo.processInfo.systemUptime
        let accepted = session.restoreCheckpointSynchronously(Data("checkpoint".utf8))
        let elapsed = ProcessInfo.processInfo.systemUptime - start

        #expect(accepted)
        #expect(elapsed >= 0.9)
        allowWriteToFinish.signal()
        session.waitForPendingOutput()
        #expect(writes.values == [
            "blocking",
            "restore:checkpoint",
        ])
    }

    @Test
    func `synchronous checkpoint restore requires attached surface`() {
        let session = makeSession(
            surfaceWrite: { _, _ in },
            surfaceRestore: { _, _ in true }
        )
        #expect(!session.restoreCheckpointSynchronously(Data("checkpoint".utf8)))
    }

    @Test
    func `synchronous checkpoint restore rejects output queue reentry`() {
        let session = makeSession(
            surfaceWrite: { _, _ in },
            surfaceRestore: { _, _ in true }
        )
        session.setSurface(testSurface(19))
        #expect(!session.restoreFromOutputQueueForTesting(Data("checkpoint".utf8)))
    }

    @Test
    func `writes and process exit preserve enqueue order`() {
        let events = LockedValues<String>()
        let session = InMemoryTerminalSession(
            write: { _ in },
            resize: { _ in },
            surfaceWrite: { _, data in
                events.append(String(decoding: data, as: UTF8.self))
            },
            processExit: { _, exitCode, runtimeMilliseconds in
                events.append("exit:\(exitCode):\(runtimeMilliseconds)")
            }
        )
        session.setSurface(testSurface(2))

        session.receive("first")
        session.receive("second")
        session.finish(exitCode: 7, runtimeMilliseconds: 42)
        session.waitForPendingOutput()

        #expect(events.values == ["first", "second", "exit:7:42"])
    }

    @Test
    func `surface teardown waits for active write and drops queued stale writes`() {
        let firstWriteStarted = DispatchSemaphore(value: 0)
        let allowFirstWriteToFinish = DispatchSemaphore(value: 0)
        let clearFinished = DispatchSemaphore(value: 0)
        let writes = LockedValues<String>()
        let surface = SendableSurface(testSurface(3))
        let session = makeSession { _, data in
            let value = String(decoding: data, as: UTF8.self)
            writes.append(value)
            if value == "first" {
                firstWriteStarted.signal()
                allowFirstWriteToFinish.wait()
            }
        }
        session.setSurface(surface.rawValue)
        session.receive("first")
        session.receive("stale")
        #expect(firstWriteStarted.wait(timeout: .now() + 1) == .success)

        DispatchQueue.global().async {
            session.clearSurface(ifMatches: surface.rawValue)
            clearFinished.signal()
        }

        let clearDeadline = ProcessInfo.processInfo.systemUptime + 1
        while session.currentSurface != nil,
              ProcessInfo.processInfo.systemUptime < clearDeadline
        {
            sched_yield()
        }
        #expect(session.currentSurface == nil)

        allowFirstWriteToFinish.signal()
        #expect(clearFinished.wait(timeout: .now() + 1) == .success)
        session.waitForPendingOutput()

        #expect(writes.values == ["first"])
    }

    @Test
    func `output arriving during teardown survives for reattachment`() {
        let firstWriteStarted = DispatchSemaphore(value: 0)
        let allowFirstWriteToFinish = DispatchSemaphore(value: 0)
        let clearFinished = DispatchSemaphore(value: 0)
        let writes = LockedValues<String>()
        let firstSurface = SendableSurface(testSurface(13))
        let session = makeSession { _, data in
            let value = String(decoding: data, as: UTF8.self)
            writes.append(value)
            if value == "blocking" {
                firstWriteStarted.signal()
                allowFirstWriteToFinish.wait()
            }
        }
        session.setSurface(firstSurface.rawValue)
        session.receive("blocking")
        #expect(firstWriteStarted.wait(timeout: .now() + 1) == .success)

        DispatchQueue.global().async {
            session.clearSurface(ifMatches: firstSurface.rawValue)
            clearFinished.signal()
        }
        let detachDeadline = ProcessInfo.processInfo.systemUptime + 1
        while session.currentSurface != nil,
              ProcessInfo.processInfo.systemUptime < detachDeadline
        {
            sched_yield()
        }
        #expect(session.currentSurface == nil)

        session.receive("during-teardown")
        allowFirstWriteToFinish.signal()
        #expect(clearFinished.wait(timeout: .now() + 1) == .success)
        session.setSurface(testSurface(14))
        session.waitForPendingOutput()

        #expect(writes.values == ["blocking", "during-teardown"])
    }

    @Test
    func `output received before attachment flushes in order`() {
        let writes = LockedValues<String>()
        let session = makeSession { _, data in
            writes.append(String(decoding: data, as: UTF8.self))
        }

        session.receive("checkpoint")
        session.receive("delta")
        session.setSurface(testSurface(6))
        session.waitForPendingOutput()

        #expect(writes.values == ["checkpoint", "delta"])
    }

    @Test
    func `pre attachment overflow fences incomplete output until checkpoint restore`() {
        let writes = LockedValues<String>()
        let session = makeSession { _, data in
            writes.append(String(decoding: data, as: UTF8.self))
        }
        let prefix = Data(
            repeating: 0x41,
            count: InMemoryTerminalSurfaceAccess.maximumPendingWriteBytes
        )

        session.receive(prefix)
        session.receive(Data([0x42]))
        session.receive("later-delta")
        session.setSurface(testSurface(7))
        session.waitForPendingOutput()

        #expect(writes.values.isEmpty)

        session.restoreCheckpointSynchronously(Data("checkpoint".utf8))
        session.receive("live-delta")
        session.waitForPendingOutput()

        #expect(writes.values == [
            "checkpoint",
            "live-delta",
        ])
    }

    @Test
    func `native checkpoint restore failure is synchronous`() {
        let events = LockedValues<String>()
        let session = makeSession(
            surfaceWrite: { _, data in
                events.append(String(decoding: data, as: UTF8.self))
            },
            surfaceRestore: { _, _ in false }
        )
        session.setSurface(testSurface(17))

        #expect(!session.restoreCheckpointSynchronously(Data("checkpoint".utf8)))
        session.receive("delta")
        session.waitForPendingOutput()

        #expect(events.values == ["delta"])
    }

    @Test
    func `checkpoint restore can retry synchronously after native failure`() {
        let events = LockedValues<String>()
        let attempts = LockedValues<Int>()
        let session = makeSession(
            surfaceWrite: { _, data in
                events.append(String(decoding: data, as: UTF8.self))
            },
            surfaceRestore: { _, data in
                attempts.append(1)
                return String(decoding: data, as: UTF8.self) == "recovery"
            }
        )
        session.setSurface(testSurface(18))

        #expect(!session.restoreCheckpointSynchronously(Data("failed".utf8)))
        #expect(session.restoreCheckpointSynchronously(Data("recovery".utf8)))
        session.receive("fresh-delta")
        session.waitForPendingOutput()

        #expect(attempts.values.count == 2)
        #expect(events.values == ["fresh-delta"])
    }

    @Test
    func `oversized authoritative checkpoint is rejected visibly to caller`() {
        let session = makeSession { _, _ in }
        let oversized = Data(
            repeating: 0x41,
            count: InMemoryTerminalSurfaceAccess.maximumPendingWriteBytes + 1
        )

        #expect(!session.restoreCheckpointSynchronously(oversized))
    }

    @Test
    func `ordinary detach resumes queued output after reattachment`() {
        let writes = LockedValues<String>()
        let session = makeSession { _, data in
            writes.append(String(decoding: data, as: UTF8.self))
        }
        let first = testSurface(11)
        session.setSurface(first)
        session.receive("before-detach")
        session.waitForPendingOutput()

        session.clearSurface(ifMatches: first)
        session.receive("while-detached")
        session.setSurface(testSurface(12))
        session.receive("after-reattach")
        session.waitForPendingOutput()

        #expect(writes.values == [
            "before-detach",
            "while-detached",
            "after-reattach",
        ])
    }

    @Test
    func `blocked session does not block another session`() {
        let firstWriteStarted = DispatchSemaphore(value: 0)
        let allowFirstWriteToFinish = DispatchSemaphore(value: 0)
        let secondWriteFinished = DispatchSemaphore(value: 0)
        let firstSession = makeSession { _, _ in
            firstWriteStarted.signal()
            allowFirstWriteToFinish.wait()
        }
        let secondSession = makeSession { _, _ in
            secondWriteFinished.signal()
        }
        firstSession.setSurface(testSurface(4))
        secondSession.setSurface(testSurface(5))

        firstSession.receive("blocked")
        #expect(firstWriteStarted.wait(timeout: .now() + 1) == .success)
        secondSession.receive("independent")

        #expect(secondWriteFinished.wait(timeout: .now() + 1) == .success)
        allowFirstWriteToFinish.signal()
        firstSession.waitForPendingOutput()
        secondSession.waitForPendingOutput()
    }
}

private func makeSession(
    surfaceWrite: @escaping InMemoryTerminalSurfaceAccess.Write,
    surfaceRestore: InMemoryTerminalSurfaceAccess.Restore? = nil
) -> InMemoryTerminalSession {
    InMemoryTerminalSession(
        write: { _ in },
        resize: { _ in },
        surfaceWrite: surfaceWrite,
        surfaceRestore: surfaceRestore ?? { surface, data in
            surfaceWrite(surface, data)
            return true
        }
    )
}

private func testSurface(_ address: Int) -> ghostty_surface_t {
    UnsafeMutableRawPointer(bitPattern: address)!
}

private struct SendableSurface: @unchecked Sendable {
    let rawValue: ghostty_surface_t

    init(_ rawValue: ghostty_surface_t) {
        self.rawValue = rawValue
    }
}

private final class LockedValues<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [Value] = []

    var values: [Value] {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }

    func append(_ value: Value) {
        lock.lock()
        storage.append(value)
        lock.unlock()
    }
}
