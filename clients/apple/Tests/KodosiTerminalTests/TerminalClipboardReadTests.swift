@testable import KodosiTerminal
import Testing

@MainActor
struct TerminalClipboardReadTests {
    @Test func `a read in another persons terminal waits for the answer of the person`() {
        let proxy = TerminalRendererDelegateProxy()
        var asked = 0
        var answers: [Bool] = []
        proxy.clipboardReadRequested = { _ in asked += 1 }

        proxy.terminalDidRequestClipboardRead { answers.append($0) }

        #expect(asked == 1)
        #expect(answers.isEmpty)
    }

    @Test func `a read in the own terminal is answered without a prompt`() {
        let proxy = TerminalRendererDelegateProxy()
        proxy.asksBeforeClipboardRead = false
        var asked = 0
        var answers: [Bool] = []
        proxy.clipboardReadRequested = { _ in asked += 1 }

        proxy.terminalDidRequestClipboardRead { answers.append($0) }

        #expect(asked == 0)
        #expect(answers == [true])
    }
}
