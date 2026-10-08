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
}
