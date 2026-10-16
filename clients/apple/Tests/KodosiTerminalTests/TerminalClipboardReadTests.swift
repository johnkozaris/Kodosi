@testable import KodosiTerminal
import Testing

@MainActor
struct TerminalClipboardReadTests {
    @Test func aReadInAnotherPersonsTerminalWaitsForTheAnswerOfThePerson() {
        let proxy = TerminalRendererDelegateProxy()
        var asked = 0
        var answers: [Bool] = []
        proxy.clipboardReadRequested = { _ in asked += 1 }

        proxy.terminalDidRequestClipboardRead { answers.append($0) }

        #expect(asked == 1)
        #expect(answers.isEmpty)
    }

    @Test func aReadInTheOwnTerminalIsAnsweredWithoutAPrompt() {
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
