import Foundation
@testable import Kodosi
import KodosiKit
import KodosiTerminalSessions
import Testing

struct TerminalInputTests {
    @Test func `a message keeps its lines and drops trailing newlines`() {
        #expect(TerminalMessage.text(from: "Plan the release\nThen ship it\n\n") == "Plan the release\nThen ship it")
        #expect(TerminalMessage.text(from: "  indented\r\n") == "  indented")
        #expect(TerminalMessage.text(from: " \n\t\n") == nil)
        #expect(TerminalMessage.text(from: "") == nil)
    }

    @Test func `only reading keeps the header in landscape and shows an agent's question`() {
        #expect(TerminalInput.reading.showsAsk && !TerminalInput.reading.showsKeys)
        #expect(TerminalInput.typing.showsKeys && !TerminalInput.typing.showsAsk)
        #expect(!TerminalInput.composing.showsKeys && !TerminalInput.composing.showsAsk)
        #expect(TerminalInput.reading.showsHeader(compact: true))
        #expect(!TerminalInput.typing.showsHeader(compact: true) && TerminalInput.typing.showsHeader(compact: false))
        #expect(!TerminalInput.composing.showsHeader(compact: true) && TerminalInput.composing.showsHeader(compact: false))
    }

    @Test func `the terminal keeps the keyboard until it loses focus`() {
        var keyboard = TerminalKeyboard(hardware: false)
        #expect(!keyboard.focusesTerminal(composing: false))
        keyboard.type()
        #expect(keyboard.focusesTerminal(composing: false))
        keyboard.terminalFocused(true)
        #expect(keyboard.input(composing: false) == .typing)
        keyboard.terminalFocused(false)
        #expect(!keyboard.focusesTerminal(composing: false))
        #expect(keyboard.input(composing: false) == .reading)
    }

    @Test func `the keyboard key focuses the terminal again after the keyboard was hidden`() {
        var keyboard = TerminalKeyboard(hardware: true)
        #expect(keyboard.focusesTerminal(composing: false))
        keyboard.terminalFocused(true)
        keyboard.hide()
        keyboard.terminalFocused(false)
        #expect(!keyboard.focusesTerminal(composing: false))
        keyboard.type()
        #expect(keyboard.focusesTerminal(composing: false))
    }

    @Test func `a sent message leaves the terminal to read with the on-screen keyboard`() {
        var keyboard = TerminalKeyboard(hardware: false)
        keyboard.type()
        keyboard.terminalFocused(true)
        keyboard.compose()
        #expect(keyboard.input(composing: false) == .composing)
        #expect(!keyboard.focusesTerminal(composing: false))
        keyboard.terminalFocused(false)
        #expect(keyboard.input(composing: true) == .composing)
        keyboard.composerClosed()
        #expect(keyboard.input(composing: false) == .reading)
        #expect(!keyboard.focusesTerminal(composing: false))
    }

    @Test func `a hardware keyboard goes back to the terminal after a message but not after hiding`() {
        var keyboard = TerminalKeyboard(hardware: true)
        keyboard.terminalFocused(true)
        keyboard.compose()
        keyboard.terminalFocused(false)
        #expect(!keyboard.focusesTerminal(composing: true))
        keyboard.composerClosed()
        #expect(keyboard.focusesTerminal(composing: false))

        keyboard.terminalFocused(true)
        keyboard.compose()
        keyboard.terminalFocused(false)
        keyboard.hide()
        keyboard.composerClosed()
        #expect(!keyboard.focusesTerminal(composing: false))
    }

    @Test func `a hardware keyboard that connects focuses the terminal`() {
        var keyboard = TerminalKeyboard(hardware: false)
        keyboard.hardwareChanged(true)
        #expect(keyboard.focusesTerminal(composing: false))
        keyboard.hide()
        keyboard.hardwareChanged(true)
        #expect(!keyboard.focusesTerminal(composing: false))

        keyboard.hardwareChanged(false)
        keyboard.type()
        keyboard.terminalFocused(true)
        keyboard.compose()
        keyboard.terminalFocused(false)
        keyboard.composerClosed()
        #expect(!keyboard.focusesTerminal(composing: false))
    }

    @Test func `a new terminal view takes the focus of the view that went away`() {
        var keyboard = TerminalKeyboard(hardware: false)
        keyboard.type()
        keyboard.terminalFocused(true)
        keyboard.terminalGone()
        #expect(keyboard.input(composing: false) == .reading)
        #expect(keyboard.focusesTerminal(composing: false))
    }

    @Test func `a message fits one input chunk`() {
        #expect(TerminalMessage.fits(String(repeating: "a", count: TerminalInputQueue.chunkBytes)))
        #expect(!TerminalMessage.fits(String(repeating: "a", count: TerminalInputQueue.chunkBytes + 1)))
    }

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

    @Test func `a paste beyond the terminal budget is refused`() {
        let queue = TerminalInputQueue(
            perSessionBudget: 1024, globalBudget: 4096,
            coalesceLimit: TerminalInputQueue.chunkBytes, busyDelays: [.milliseconds(1)],
        )
        let failures = TerminalFailureRecorder()
        queue.enqueue(Data(repeating: 1, count: 2048), sessionId: "s1", coalescingToken: 1, send: { _ in
            Issue.record("An oversized paste must not be sent")
            return Int32(KODOSI_FFI_OK)
        }, onFailure: { failures.append($0) })
        #expect(failures.values == [.budgetExceeded(perSession: true)])
        #expect(TerminalInputFailure.budgetExceeded(perSession: true).userMessage.isEmpty == false)
    }
}

final nonisolated class TerminalChunkRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Data] = []

    func append(_ data: Data) {
        lock.withLock { values.append(data) }
    }

    var chunks: [Data] {
        lock.withLock { values }
    }

    var joined: Data {
        lock.withLock { values.reduce(into: Data()) { $0.append($1) } }
    }

    func waitFor(bytes: Int) async {
        for _ in 0 ..< 500 where joined.count < bytes {
            try? await Task.sleep(for: .milliseconds(2))
        }
    }
}

final nonisolated class TerminalFailureRecorder: @unchecked Sendable {
    private let lock = NSLock()
    private var failures: [TerminalInputFailure] = []

    func append(_ failure: TerminalInputFailure) {
        lock.withLock { failures.append(failure) }
    }

    var values: [TerminalInputFailure] {
        lock.withLock { failures }
    }
}
