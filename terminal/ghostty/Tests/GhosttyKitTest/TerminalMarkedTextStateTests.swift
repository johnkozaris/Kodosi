import Foundation
@testable import GhosttyTerminal
import Testing

struct TerminalMarkedTextStateTests {
    @Test
    func `set marked text clamps selection into document`() {
        var state = TerminalMarkedTextState()
        state.setMarkedText("abcd", selectedRange: NSRange(location: 99, length: 3))

        #expect(state.selectedRange == NSRange(location: 4, length: 0))
        #expect(state.documentLength == 4)
    }

    @Test
    func `text in range returns substring and empty caret slice`() {
        var state = TerminalMarkedTextState()
        state.setMarkedText("中文abc", selectedRange: NSRange(location: 2, length: 0))

        #expect(state.text(in: NSRange(location: 0, length: 2)) == "中文")
        #expect(state.text(in: NSRange(location: 2, length: 0)) == "")
        #expect(state.text(in: NSRange(location: 99, length: 1)) == nil)
    }

    @Test
    func `delete backward removes whole composed characters before the caret`() {
        var state = TerminalMarkedTextState()
        state.setMarkedText("a👍🏽", selectedRange: NSRange(location: 5, length: 0))

        let deletedEmoji = state.deleteBackward()
        #expect(deletedEmoji)
        #expect(state.text == "a")
        #expect(state.selectedRange == NSRange(location: 1, length: 0))

        let deletedLetter = state.deleteBackward()
        #expect(deletedLetter)
        #expect(state.text == nil)
        let deletedNothing = state.deleteBackward()
        #expect(!deletedNothing)
    }

    @Test
    func `delete backward removes the selected marked range`() {
        var state = TerminalMarkedTextState()
        state.setMarkedText("かなabc", selectedRange: NSRange(location: 0, length: 2))

        let deletedSelection = state.deleteBackward()
        #expect(deletedSelection)
        #expect(state.text == "abc")
        #expect(state.selectedRange == NSRange(location: 0, length: 0))
    }
}
