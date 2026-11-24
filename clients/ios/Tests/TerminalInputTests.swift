import Foundation
@testable import Kodosi
import KodosiKit
import KodosiTerminalSessions
import Synchronization
import Testing

struct TerminalInputTests {
    @Test func `typing is delivered in order in bounded chunks`() async {
        let queue = TerminalInputQueue(
            perSessionBudget: 1 << 20, globalBudget: 1 << 21,
            coalesceLimit: TerminalInputQueue.chunkBytes, busyDelays: [.milliseconds(1)],
        )
        let sent = TerminalChunkRecorder()
        var expected = Data()
        for index in 0 ..< 6 {
            let piece = Data(repeating: UInt8(index), count: 40 * 1024)
            expected.append(piece)
            queue.enqueue(piece, sessionId: "s1", coalescingToken: 1, send: { data in
                sent.append(data)
                return Int32(KODOSI_FFI_OK)
            }, onFailure: { _ in Issue.record("Typing must not fail") })
        }
        await sent.waitFor(bytes: expected.count)
        #expect(sent.joined == expected)
        #expect(sent.chunks.allSatisfy { $0.count <= TerminalInputQueue.chunkBytes })
    }

    @Test func `a busy runtime gets the same chunk again without loss or repeat`() async {
        let queue = TerminalInputQueue(
            perSessionBudget: 1 << 20, globalBudget: 1 << 21,
            coalesceLimit: TerminalInputQueue.chunkBytes, busyDelays: [.milliseconds(1)],
        )
        let sent = TerminalChunkRecorder()
        let busy = TerminalChunkRecorder()
        queue.enqueue(Data("1\r".utf8), sessionId: "s1", coalescingToken: 1, send: { data in
            if busy.chunks.isEmpty {
                busy.append(data)
                return Int32(KODOSI_FFI_BUSY)
            }
            sent.append(data)
            return Int32(KODOSI_FFI_OK)
        }, onFailure: { _ in Issue.record("A busy runtime must be retried") })
        await sent.waitFor(bytes: 2)
        #expect(sent.joined == Data("1\r".utf8))
        #expect(busy.chunks.count == 1)
    }
}

final nonisolated class TerminalChunkRecorder: Sendable {
    private let values = Mutex<[Data]>([])

    func append(_ data: Data) {
        values.withLock { $0.append(data) }
    }

    var chunks: [Data] {
        values.withLock { $0 }
    }

    var joined: Data {
        values.withLock { $0.reduce(into: Data()) { $0.append($1) } }
    }

    func waitFor(bytes: Int) async {
        for _ in 0 ..< 500 where joined.count < bytes {
            try? await Task.sleep(for: .milliseconds(2))
        }
    }
}
